//! HTTP layer: status mapping, retry, PackOutcome, SignatureOutcome.
//!
//! This is the ONLY place for status code handling. client/mod.rs never
//! interprets status codes.

use std::time::Duration;

use reqwest::header::{AUTHORIZATION, IF_NONE_MATCH};
use reqwest::StatusCode;
use tracing::{debug, warn};

use crate::auth::TokenProvider;
use crate::error::{RegistryError, RegistryResult};
use crate::types::{PackHeaders, RegistryConfig};

use super::helpers::{parse_pack_url, parse_revocation_body};

/// Outcome of pack fetch (exact behavior parity: Ok(None) only for 304).
#[derive(Debug)]
pub(crate) enum PackOutcome {
    NotModified,
    Fetched(PackFetched),
}

#[derive(Debug)]
pub(crate) struct PackFetched {
    pub headers: PackHeaders,
    pub content: String,
}

/// Outcome of signature sidecar fetch.
#[derive(Debug)]
pub(crate) enum SignatureOutcome {
    Missing,
    Present(String),
}

/// HTTP backend for making requests (holds reqwest client, auth, config).
#[derive(Debug, Clone)]
pub(crate) struct HttpBackend {
    pub(crate) client: reqwest::Client,
    pub(crate) base_url: String,
    pub(crate) token_provider: TokenProvider,
    pub(crate) config: RegistryConfig,
}

impl HttpBackend {
    /// Fetch pack content; returns PackOutcome (NotModified only for 304).
    pub(crate) async fn fetch_pack(
        &self,
        url: &str,
        etag: Option<&str>,
    ) -> RegistryResult<PackOutcome> {
        let response = self.request(reqwest::Method::GET, url, etag).await?;

        if response.status() == StatusCode::NOT_MODIFIED {
            debug!("pack not modified (304)");
            return Ok(PackOutcome::NotModified);
        }

        let headers = PackHeaders::from_headers(response.headers());
        let content = response.text().await.map_err(|e| RegistryError::Network {
            message: format!("failed to read response body: {}", e),
        })?;

        Ok(PackOutcome::Fetched(PackFetched { headers, content }))
    }

    /// Fetch signature sidecar; 404 => Missing, 200 => Present.
    pub(crate) async fn fetch_signature_optional(
        &self,
        url: &str,
    ) -> RegistryResult<SignatureOutcome> {
        match self.request(reqwest::Method::GET, url, None).await {
            Ok(response) => {
                let text = response.text().await.map_err(|e| RegistryError::Network {
                    message: format!("failed to read signature body: {}", e),
                })?;
                Ok(SignatureOutcome::Present(text))
            }
            Err(RegistryError::NotFound { .. }) => {
                debug!("signature sidecar not found (pack may be unsigned)");
                Ok(SignatureOutcome::Missing)
            }
            Err(e) => Err(e),
        }
    }

    /// Make a single request (used by fetch_pack, fetch_signature_optional, and for JSON/HEAD).
    pub(crate) async fn request(
        &self,
        method: reqwest::Method,
        url: &str,
        etag: Option<&str>,
    ) -> RegistryResult<reqwest::Response> {
        use rand::Rng;

        let mut retries = 0;
        let max_retries = self.config.max_retries;

        loop {
            let result = self.request_once(method.clone(), url, etag).await;

            match result {
                Ok(response) => return Ok(response),
                Err(e) if e.is_retryable() && retries < max_retries => {
                    retries += 1;

                    let backoff = match &e {
                        RegistryError::RateLimited {
                            retry_after: Some(retry_after),
                        } => {
                            let thirty_sec = Duration::from_secs(30);
                            let capped = if *retry_after > thirty_sec {
                                thirty_sec
                            } else {
                                *retry_after
                            };
                            let base_ms = capped.as_millis() as u64;
                            let jitter_factor: f64 =
                                rand::thread_rng().gen_range(0.9_f64..=1.1_f64);
                            let jittered_ms = ((base_ms as f64) * jitter_factor).round() as u64;
                            Duration::from_millis(jittered_ms.max(100))
                        }
                        _ => {
                            let base_backoff = Duration::from_secs(1 << retries);
                            let base_backoff = base_backoff.min(Duration::from_secs(30));
                            let jittered_ms =
                                rand::thread_rng().gen_range(0..=base_backoff.as_millis() as u64);
                            Duration::from_millis(jittered_ms.max(10))
                        }
                    };

                    warn!(
                        error = %e,
                        retry = retries,
                        max_retries = max_retries,
                        backoff_ms = backoff.as_millis(),
                        "retrying request"
                    );

                    tokio::time::sleep(backoff).await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    async fn request_once(
        &self,
        method: reqwest::Method,
        url: &str,
        etag: Option<&str>,
    ) -> RegistryResult<reqwest::Response> {
        let mut request = self.client.request(method, url);

        if let Some(token) = self.token_provider.get_token().await? {
            request = request.header(AUTHORIZATION, format!("Bearer {}", token));
        }

        if let Some(etag) = etag {
            request = request.header(IF_NONE_MATCH, etag);
        }

        let response = request.send().await?;
        let status = response.status();

        match status.as_u16() {
            200..=299 | 304 => Ok(response),

            401 => Err(RegistryError::Unauthorized {
                message: "invalid or expired token".to_string(),
            }),

            404 => {
                let (name, version) = parse_pack_url(url);
                Err(RegistryError::NotFound { name, version })
            }

            410 => {
                let (name, version) = parse_pack_url(url);
                let header_reason = response
                    .headers()
                    .get("x-revocation-reason")
                    .and_then(|v| v.to_str().ok())
                    .map(String::from);

                let body: Option<String> = response.text().await.ok();
                let (reason, safe_version) = if let Some(ref body_text) = body {
                    parse_revocation_body(body_text, header_reason)
                } else {
                    (
                        header_reason.unwrap_or_else(|| "no reason provided".to_string()),
                        None,
                    )
                };

                Err(RegistryError::Revoked {
                    name,
                    version,
                    reason,
                    safe_version,
                })
            }

            429 => {
                let retry_after = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .map(Duration::from_secs);

                Err(RegistryError::RateLimited { retry_after })
            }

            _ => {
                let message = response.text().await.unwrap_or_else(|_| status.to_string());
                Err(RegistryError::Network {
                    message: format!("HTTP {}: {}", status.as_u16(), message),
                })
            }
        }
    }
}
