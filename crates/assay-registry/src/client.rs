//! HTTP client for the pack registry.

use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, IF_NONE_MATCH, USER_AGENT};
use tracing::{debug, warn};

use crate::auth::TokenProvider;
use crate::digest::compute_canonical_or_raw_digest;
use crate::error::{RegistryError, RegistryResult};
use crate::types::{
    DsseEnvelope, FetchResult, KeysManifest, PackHeaders, PackMeta, RegistryConfig,
    VersionsResponse,
};

/// User agent for registry requests.
const USER_AGENT_VALUE: &str = concat!("assay-registry/", env!("CARGO_PKG_VERSION"));

/// Registry client for fetching packs.
#[derive(Debug, Clone)]
pub struct RegistryClient {
    /// HTTP client.
    client: reqwest::Client,

    /// Base URL for the registry.
    base_url: String,

    /// Token provider for authentication.
    token_provider: TokenProvider,

    /// Configuration.
    config: RegistryConfig,
}

impl RegistryClient {
    /// Create a new registry client.
    pub fn new(config: RegistryConfig) -> RegistryResult<Self> {
        let token_provider = config
            .token
            .as_ref()
            .map(TokenProvider::static_token)
            .unwrap_or_else(TokenProvider::from_env);

        Self::with_token_provider(config, token_provider)
    }

    /// Create a client with a custom token provider.
    pub fn with_token_provider(
        config: RegistryConfig,
        token_provider: TokenProvider,
    ) -> RegistryResult<Self> {
        let mut default_headers = HeaderMap::new();
        default_headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE));

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .default_headers(default_headers)
            .build()
            .map_err(|e| RegistryError::Network {
                message: format!("failed to create HTTP client: {}", e),
            })?;

        // Normalize base URL (remove trailing slash)
        let base_url = config.url.trim_end_matches('/').to_string();

        Ok(Self {
            client,
            base_url,
            token_provider,
            config,
        })
    }

    /// Create a client from environment variables.
    pub fn from_env() -> RegistryResult<Self> {
        Self::new(RegistryConfig::from_env())
    }

    /// Get pack metadata (HEAD request).
    ///
    /// Returns metadata without downloading content.
    pub async fn get_pack_meta(&self, name: &str, version: &str) -> RegistryResult<PackMeta> {
        let url = format!("{}/packs/{}/{}", self.base_url, name, version);
        debug!(url = %url, "fetching pack metadata");

        let response = self.request(reqwest::Method::HEAD, &url, None).await?;

        let headers = PackHeaders::from_headers(response.headers());

        Ok(PackMeta {
            name: name.to_string(),
            version: version.to_string(),
            description: None,
            digest: headers.digest.unwrap_or_default(),
            size: headers.content_length,
            published_at: None,
            signed: headers.signature.is_some(),
            key_id: headers.key_id,
            deprecated: false,
            deprecation_message: None,
        })
    }

    /// Fetch pack content.
    ///
    /// Returns the pack YAML content and headers.
    pub async fn fetch_pack(
        &self,
        name: &str,
        version: &str,
        etag: Option<&str>,
    ) -> RegistryResult<Option<FetchResult>> {
        let url = format!("{}/packs/{}/{}", self.base_url, name, version);
        debug!(url = %url, etag = ?etag, "fetching pack content");

        let response = self.request(reqwest::Method::GET, &url, etag).await?;

        // Check for 304 Not Modified
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            debug!("pack not modified (304)");
            return Ok(None);
        }

        let headers = PackHeaders::from_headers(response.headers());
        let content = response.text().await.map_err(|e| RegistryError::Network {
            message: format!("failed to read response body: {}", e),
        })?;

        // Compute digest of content
        let computed_digest = compute_digest(&content);

        Ok(Some(FetchResult {
            content,
            headers,
            computed_digest,
        }))
    }

    /// List available versions for a pack.
    pub async fn list_versions(&self, name: &str) -> RegistryResult<VersionsResponse> {
        let url = format!("{}/packs/{}/versions", self.base_url, name);
        debug!(url = %url, "listing pack versions");

        let response = self.request(reqwest::Method::GET, &url, None).await?;

        response
            .json()
            .await
            .map_err(|e| RegistryError::InvalidResponse {
                message: format!("failed to parse versions response: {}", e),
            })
    }

    /// Fetch the keys manifest.
    pub async fn fetch_keys(&self) -> RegistryResult<KeysManifest> {
        let url = format!("{}/keys", self.base_url);
        debug!(url = %url, "fetching keys manifest");

        let response = self.request(reqwest::Method::GET, &url, None).await?;

        response
            .json()
            .await
            .map_err(|e| RegistryError::InvalidResponse {
                message: format!("failed to parse keys manifest: {}", e),
            })
    }

    /// Fetch signature from sidecar endpoint (SPEC §7.3).
    ///
    /// The signature is delivered separately from the pack content to avoid
    /// header size limits. The sidecar returns a DSSE envelope.
    ///
    /// Endpoint: GET /packs/{name}/{version}.sig
    pub async fn fetch_signature(
        &self,
        name: &str,
        version: &str,
    ) -> RegistryResult<Option<DsseEnvelope>> {
        let url = format!("{}/packs/{}/{}.sig", self.base_url, name, version);
        debug!(url = %url, "fetching signature from sidecar");

        let response = match self.request(reqwest::Method::GET, &url, None).await {
            Ok(response) => response,
            Err(RegistryError::NotFound { .. }) => {
                // Signature sidecar may not exist for unsigned packs
                debug!("signature sidecar not found (pack may be unsigned)");
                return Ok(None);
            }
            Err(e) => return Err(e),
        };

        let envelope: DsseEnvelope =
            response
                .json()
                .await
                .map_err(|e| RegistryError::InvalidResponse {
                    message: format!("failed to parse signature envelope: {}", e),
                })?;

        Ok(Some(envelope))
    }

    /// Fetch pack with signature from sidecar (recommended for signed packs).
    ///
    /// This method fetches both the pack content and its signature in parallel,
    /// avoiding header size limits from DSSE-in-header.
    pub async fn fetch_pack_with_signature(
        &self,
        name: &str,
        version: &str,
        etag: Option<&str>,
    ) -> RegistryResult<Option<(FetchResult, Option<DsseEnvelope>)>> {
        // Fetch pack content
        let pack_result = self.fetch_pack(name, version, etag).await?;

        let fetch = match pack_result {
            Some(f) => f,
            None => return Ok(None), // 304 Not Modified
        };

        // Fetch signature from sidecar (don't fail if missing)
        let signature = self.fetch_signature(name, version).await.ok().flatten();

        Ok(Some((fetch, signature)))
    }

    /// Make an authenticated request with retry and rate limit handling.
    ///
    /// Uses exponential backoff with full jitter to prevent thundering herd.
    /// Note: Server-specified Retry-After is respected without jitter.
    async fn request(
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

                    // Calculate backoff - apply jitter to avoid synchronized retries
                    let backoff = match &e {
                        RegistryError::RateLimited {
                            retry_after: Some(retry_after),
                        } => {
                            // Server specified delay - respect it approximately, but add small jitter
                            let capped = (*retry_after).min(Duration::from_secs(30));
                            let base_ms = capped.as_millis() as u64;
                            // Apply small jitter of ±10% to avoid thundering herd on 429
                            let jitter_factor: f64 =
                                rand::thread_rng().gen_range(0.9_f64..=1.1_f64);
                            let jittered_ms = ((base_ms as f64) * jitter_factor).round() as u64;
                            Duration::from_millis(jittered_ms.max(100)) // At least 100ms
                        }
                        _ => {
                            // Exponential backoff with full jitter for other errors
                            let base_backoff = Duration::from_secs(1 << retries);
                            let base_backoff = base_backoff.min(Duration::from_secs(30));

                            // Apply full jitter: sleep = rand(0..base_backoff)
                            // This prevents thundering herd when many clients retry simultaneously
                            let jittered_ms =
                                rand::thread_rng().gen_range(0..=base_backoff.as_millis() as u64);
                            Duration::from_millis(jittered_ms.max(10)) // At least 10ms
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

    /// Make a single request without retry.
    async fn request_once(
        &self,
        method: reqwest::Method,
        url: &str,
        etag: Option<&str>,
    ) -> RegistryResult<reqwest::Response> {
        let mut request = self.client.request(method, url);

        // Add authentication if available
        if let Some(token) = self.token_provider.get_token().await? {
            request = request.header(AUTHORIZATION, format!("Bearer {}", token));
        }

        // Add ETag for conditional requests
        if let Some(etag) = etag {
            request = request.header(IF_NONE_MATCH, etag);
        }

        let response = request.send().await?;
        let status = response.status();

        // Handle error status codes
        match status.as_u16() {
            200..=299 | 304 => Ok(response),

            401 => Err(RegistryError::Unauthorized {
                message: "invalid or expired token".to_string(),
            }),

            404 => {
                // Extract name and version from URL for better error message
                let (name, version) = parse_pack_url(url);
                Err(RegistryError::NotFound { name, version })
            }

            410 => {
                // Pack revoked - parse body for detailed info, fallback to header
                let (name, version) = parse_pack_url(url);

                // Try to extract reason from header first (fast path)
                let header_reason = response
                    .headers()
                    .get("x-revocation-reason")
                    .and_then(|v| v.to_str().ok())
                    .map(String::from);

                // Parse body for structured revocation info
                // Body format: {"reason": "...", "safe_version": "1.0.1"}
                let body = response.text().await.ok();
                let (reason, safe_version) = if let Some(body_text) = body {
                    parse_revocation_body(&body_text, header_reason)
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
                // Rate limited - extract Retry-After
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

    /// Get the base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Check if authenticated.
    pub fn is_authenticated(&self) -> bool {
        self.token_provider.is_authenticated()
    }
}

/// Compute canonical digest of content per SPEC §6.2.
///
/// Uses JCS canonicalization for valid YAML, falls back to raw SHA-256 for
/// non-YAML content (e.g., error responses).
fn compute_digest(content: &str) -> String {
    compute_canonical_or_raw_digest(content, |_| {})
}

/// Parse pack name and version from URL.
fn parse_pack_url(url: &str) -> (String, String) {
    // URL format: .../packs/{name}/{version}
    let parts: Vec<&str> = url.split('/').collect();
    let len = parts.len();

    if len >= 2 {
        (
            parts.get(len - 2).unwrap_or(&"unknown").to_string(),
            parts.get(len - 1).unwrap_or(&"unknown").to_string(),
        )
    } else {
        ("unknown".to_string(), "unknown".to_string())
    }
}

/// Parse 410 revocation response body.
///
/// Expected format: `{"reason": "...", "safe_version": "1.0.1"}`
/// Falls back to header_reason if body parsing fails.
fn parse_revocation_body(body: &str, header_reason: Option<String>) -> (String, Option<String>) {
    // Try to parse as JSON
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
        let reason = json
            .get("reason")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or(header_reason)
            .unwrap_or_else(|| "no reason provided".to_string());

        let safe_version = json
            .get("safe_version")
            .and_then(|v| v.as_str())
            .map(String::from);

        (reason, safe_version)
    } else {
        // Not JSON - use body as reason if header not available
        let reason = header_reason.unwrap_or_else(|| {
            if body.is_empty() {
                "no reason provided".to_string()
            } else {
                body.chars().take(200).collect() // Limit reason length
            }
        });
        (reason, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonicalize::compute_canonical_digest;
    use crate::verify::compute_digest as verify_compute_digest;

    #[test]
    fn test_compute_digest_canonical() {
        // Valid YAML uses canonical JCS digest
        let content = "name: test\nversion: \"1.0.0\"";
        let digest = compute_digest(content);
        assert!(digest.starts_with("sha256:"));
        assert_eq!(digest.len(), 7 + 64);

        // Should match the canonical digest
        let expected = compute_canonical_digest(content).unwrap();
        assert_eq!(digest, expected);
    }

    #[test]
    fn test_compute_digest_non_yaml_fallback() {
        // Invalid YAML falls back to raw digest
        let content = "this is not: valid: yaml: [[";
        let digest = compute_digest(content);
        assert!(digest.starts_with("sha256:"));
        assert_eq!(digest.len(), 7 + 64);
    }

    #[test]
    fn test_compute_digest_parity_with_verify_module() {
        let canonical_yaml = "name: test\nversion: \"1.0.0\"\nkind: compliance";
        let invalid_yaml = "this is not: valid: yaml: [[";

        assert_eq!(
            compute_digest(canonical_yaml),
            verify_compute_digest(canonical_yaml)
        );
        assert_eq!(
            compute_digest(invalid_yaml),
            verify_compute_digest(invalid_yaml)
        );
    }

    #[test]
    fn test_compute_digest_has_lowercase_hex_shape() {
        let digest = compute_digest("name: test\nversion: \"1.0.0\"");
        assert!(digest.starts_with("sha256:"));
        assert_eq!(digest.len(), 7 + 64);
        assert!(digest[7..]
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn test_parse_pack_url() {
        let url = "https://registry.getassay.dev/v1/packs/eu-ai-act/1.2.0";
        let (name, version) = parse_pack_url(url);
        assert_eq!(name, "eu-ai-act");
        assert_eq!(version, "1.2.0");
    }

    #[test]
    fn test_config_from_env_defaults() {
        // Clear env vars
        std::env::remove_var("ASSAY_REGISTRY_URL");
        std::env::remove_var("ASSAY_REGISTRY_TOKEN");

        let config = RegistryConfig::from_env();
        assert_eq!(config.url, "https://registry.getassay.dev/v1");
        assert!(config.token.is_none());
        assert!(!config.allow_unsigned);
    }

    #[test]
    fn test_config_builder() {
        let config = RegistryConfig::default()
            .with_url("https://custom.registry.dev/v1")
            .with_token("my-token")
            .with_allow_unsigned(true);

        assert_eq!(config.url, "https://custom.registry.dev/v1");
        assert_eq!(config.token, Some("my-token".to_string()));
        assert!(config.allow_unsigned);
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn create_test_client(mock_server: &MockServer) -> RegistryClient {
        let config = RegistryConfig::default()
            .with_url(mock_server.uri())
            .with_token("test-token");
        RegistryClient::new(config).expect("failed to create client")
    }

    #[tokio::test]
    async fn test_fetch_pack_success() {
        let mock_server = MockServer::start().await;

        // Use valid YAML that will parse correctly for canonical digest
        let pack_yaml = "name: test-pack\nversion: \"1.0.0\"\nrules: []";
        let expected_digest = compute_digest(pack_yaml);

        Mock::given(method("GET"))
            .and(path("/packs/test-pack/1.0.0"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(pack_yaml)
                    .insert_header("x-pack-digest", expected_digest.as_str())
                    .insert_header("etag", "\"abc123\""),
            )
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let result = client
            .fetch_pack("test-pack", "1.0.0", None)
            .await
            .expect("fetch failed");

        let fetch = result.expect("expected Some");
        assert_eq!(fetch.content, pack_yaml);
        assert_eq!(fetch.computed_digest, expected_digest);
        assert_eq!(fetch.headers.etag, Some("\"abc123\"".to_string()));
    }

    #[tokio::test]
    async fn test_fetch_pack_304_not_modified() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/packs/test-pack/1.0.0"))
            .and(header("if-none-match", "\"abc123\""))
            .respond_with(ResponseTemplate::new(304))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let result = client
            .fetch_pack("test-pack", "1.0.0", Some("\"abc123\""))
            .await
            .expect("fetch failed");

        assert!(result.is_none(), "expected None for 304");
    }

    #[tokio::test]
    async fn test_fetch_pack_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/packs/nonexistent/1.0.0"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let result = client.fetch_pack("nonexistent", "1.0.0", None).await;

        assert!(matches!(result, Err(RegistryError::NotFound { .. })));
    }

    #[tokio::test]
    async fn test_fetch_pack_unauthorized() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/packs/private-pack/1.0.0"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let result = client.fetch_pack("private-pack", "1.0.0", None).await;

        assert!(matches!(result, Err(RegistryError::Unauthorized { .. })));
    }

    #[tokio::test]
    async fn test_fetch_pack_revoked_header_only() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/packs/revoked-pack/1.0.0"))
            .respond_with(
                ResponseTemplate::new(410)
                    .insert_header("x-revocation-reason", "security vulnerability"),
            )
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let result = client.fetch_pack("revoked-pack", "1.0.0", None).await;

        match result {
            Err(RegistryError::Revoked {
                name,
                version,
                reason,
                safe_version,
            }) => {
                assert_eq!(name, "revoked-pack");
                assert_eq!(version, "1.0.0");
                assert_eq!(reason, "security vulnerability");
                assert!(
                    safe_version.is_none(),
                    "Header-only should have no safe_version"
                );
            }
            _ => panic!("expected Revoked error"),
        }
    }

    #[tokio::test]
    async fn test_fetch_pack_revoked_with_body() {
        // P1 fix: Parse body for safe_version
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/packs/revoked-pack/1.0.0"))
            .respond_with(ResponseTemplate::new(410).set_body_json(serde_json::json!({
                "reason": "critical CVE",
                "safe_version": "1.0.1"
            })))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let result = client.fetch_pack("revoked-pack", "1.0.0", None).await;

        match result {
            Err(RegistryError::Revoked {
                name,
                version,
                reason,
                safe_version,
            }) => {
                assert_eq!(name, "revoked-pack");
                assert_eq!(version, "1.0.0");
                assert_eq!(reason, "critical CVE");
                assert_eq!(safe_version, Some("1.0.1".to_string()));
            }
            _ => panic!("expected Revoked error with safe_version"),
        }
    }

    #[tokio::test]
    async fn test_rate_limiting_with_retry_after() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/packs/rate-limited/1.0.0"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "5"))
            .mount(&mock_server)
            .await;

        // Create client with no retries to test immediate error
        let config = RegistryConfig {
            url: mock_server.uri(),
            token: Some("test-token".to_string()),
            max_retries: 0,
            ..Default::default()
        };
        let client = RegistryClient::new(config).expect("failed to create client");
        let result = client.fetch_pack("rate-limited", "1.0.0", None).await;

        match result {
            Err(RegistryError::RateLimited { retry_after }) => {
                assert_eq!(retry_after, Some(Duration::from_secs(5)));
            }
            _ => panic!("expected RateLimited error"),
        }
    }

    #[tokio::test]
    async fn test_list_versions() {
        let mock_server = MockServer::start().await;

        let versions_json = serde_json::json!({
            "name": "test-pack",
            "versions": [
                {"version": "1.2.0", "digest": "sha256:abc123", "deprecated": false},
                {"version": "1.1.0", "digest": "sha256:def456", "deprecated": false},
                {"version": "1.0.0", "digest": "sha256:789abc", "deprecated": true}
            ]
        });

        Mock::given(method("GET"))
            .and(path("/packs/test-pack/versions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&versions_json))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let response = client
            .list_versions("test-pack")
            .await
            .expect("list versions failed");

        assert_eq!(response.name, "test-pack");
        assert_eq!(response.versions.len(), 3);
        assert_eq!(response.versions[0].version, "1.2.0");
        assert!(response.versions[2].deprecated);
    }

    #[tokio::test]
    async fn test_get_pack_meta() {
        let mock_server = MockServer::start().await;

        Mock::given(method("HEAD"))
            .and(path("/packs/test-pack/1.0.0"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("x-pack-digest", "sha256:abc123")
                    .insert_header("x-pack-signature", "dGVzdC1zaWduYXR1cmU=")
                    .insert_header("x-pack-key-id", "sha256:keyid123")
                    .insert_header("content-length", "1024"),
            )
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let meta = client
            .get_pack_meta("test-pack", "1.0.0")
            .await
            .expect("get meta failed");

        assert_eq!(meta.name, "test-pack");
        assert_eq!(meta.version, "1.0.0");
        assert_eq!(meta.digest, "sha256:abc123");
        assert!(meta.signed);
        assert_eq!(meta.key_id, Some("sha256:keyid123".to_string()));
        assert_eq!(meta.size, Some(1024));
    }

    #[tokio::test]
    async fn test_fetch_keys_manifest() {
        let mock_server = MockServer::start().await;

        let keys_json = serde_json::json!({
            "version": 1,
            "keys": [
                {
                    "key_id": "sha256:abc123",
                    "algorithm": "Ed25519",
                    "public_key": "dGVzdC1wdWJsaWMta2V5",
                    "description": "Production signing key"
                }
            ]
        });

        Mock::given(method("GET"))
            .and(path("/keys"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&keys_json))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let manifest = client.fetch_keys().await.expect("fetch keys failed");

        assert_eq!(manifest.version, 1);
        assert_eq!(manifest.keys.len(), 1);
        assert_eq!(manifest.keys[0].key_id, "sha256:abc123");
        assert_eq!(manifest.keys[0].algorithm, "Ed25519");
    }

    #[tokio::test]
    async fn test_authentication_header() {
        let mock_server = MockServer::start().await;

        // This mock requires the auth header
        Mock::given(method("GET"))
            .and(path("/packs/test/1.0.0"))
            .and(header("authorization", "Bearer secret-token"))
            .respond_with(ResponseTemplate::new(200).set_body_string("content"))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = RegistryConfig::default()
            .with_url(mock_server.uri())
            .with_token("secret-token");
        let client = RegistryClient::new(config).expect("failed to create client");

        let result = client.fetch_pack("test", "1.0.0", None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_no_auth_when_no_token() {
        let mock_server = MockServer::start().await;

        // This mock should NOT have auth header
        Mock::given(method("GET"))
            .and(path("/packs/public/1.0.0"))
            .respond_with(ResponseTemplate::new(200).set_body_string("content"))
            .mount(&mock_server)
            .await;

        let config = RegistryConfig {
            url: mock_server.uri(),
            token: None,
            ..Default::default()
        };
        let client = RegistryClient::new(config).expect("failed to create client");

        assert!(!client.is_authenticated());
        let result = client.fetch_pack("public", "1.0.0", None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_user_agent_header() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/packs/test/1.0.0"))
            .and(header("user-agent", USER_AGENT_VALUE))
            .respond_with(ResponseTemplate::new(200).set_body_string("content"))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let _ = client.fetch_pack("test", "1.0.0", None).await;
    }

    #[tokio::test]
    async fn test_fetch_signature_sidecar() {
        let mock_server = MockServer::start().await;

        let envelope = serde_json::json!({
            "payloadType": "application/vnd.assay.pack+yaml;v=1",
            "payload": "dGVzdCBwYXlsb2Fk",
            "signatures": [{
                "keyid": "sha256:abc123",
                "sig": "dGVzdCBzaWduYXR1cmU="
            }]
        });

        Mock::given(method("GET"))
            .and(path("/packs/signed-pack/1.0.0.sig"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&envelope))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let result = client
            .fetch_signature("signed-pack", "1.0.0")
            .await
            .expect("fetch signature failed");

        let sig = result.expect("expected Some");
        assert_eq!(sig.payload_type, "application/vnd.assay.pack+yaml;v=1");
        assert_eq!(sig.signatures.len(), 1);
        assert_eq!(sig.signatures[0].key_id, "sha256:abc123");
    }

    #[tokio::test]
    async fn test_fetch_signature_sidecar_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/packs/unsigned-pack/1.0.0.sig"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let result = client
            .fetch_signature("unsigned-pack", "1.0.0")
            .await
            .expect("fetch signature should not error on 404");

        assert!(result.is_none(), "expected None for unsigned pack");
    }

    #[tokio::test]
    async fn test_fetch_pack_with_signature() {
        let mock_server = MockServer::start().await;

        let pack_yaml = "name: signed-pack\nversion: \"1.0.0\"";
        let expected_digest = compute_digest(pack_yaml);

        // Mock pack endpoint
        Mock::given(method("GET"))
            .and(path("/packs/signed-pack/1.0.0"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(pack_yaml)
                    .insert_header("x-pack-digest", expected_digest.as_str()),
            )
            .mount(&mock_server)
            .await;

        // Mock signature sidecar
        let envelope = serde_json::json!({
            "payloadType": "application/vnd.assay.pack+yaml;v=1",
            "payload": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, pack_yaml),
            "signatures": [{
                "keyid": "sha256:key123",
                "sig": "dGVzdCBzaWduYXR1cmU="
            }]
        });

        Mock::given(method("GET"))
            .and(path("/packs/signed-pack/1.0.0.sig"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&envelope))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let result = client
            .fetch_pack_with_signature("signed-pack", "1.0.0", None)
            .await
            .expect("fetch failed");

        let (fetch, sig) = result.expect("expected Some");
        assert_eq!(fetch.content, pack_yaml);
        assert!(sig.is_some());
        assert_eq!(sig.unwrap().signatures[0].key_id, "sha256:key123");
    }

    #[tokio::test]
    async fn test_commercial_pack_signature_required_via_sidecar_only() {
        // Scenario: Commercial pack has NO X-Pack-Signature header
        // Client MUST fetch signature from sidecar endpoint
        let mock_server = MockServer::start().await;

        let pack_yaml = "name: commercial-pack\nversion: \"1.0.0\"\nlicense: commercial";
        let expected_digest = compute_digest(pack_yaml);

        // Pack endpoint returns NO X-Pack-Signature header (header too large or policy)
        Mock::given(method("GET"))
            .and(path("/packs/commercial-pack/1.0.0"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(pack_yaml)
                    .insert_header("x-pack-digest", expected_digest.as_str())
                    .insert_header("x-pack-license", "LicenseRef-Assay-Enterprise-1.0")
                    // NOTE: No X-Pack-Signature header!
                    .insert_header(
                        "x-pack-signature-endpoint",
                        "/packs/commercial-pack/1.0.0.sig",
                    ),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        // Signature MUST be fetched from sidecar
        let envelope = serde_json::json!({
            "payloadType": "application/vnd.assay.pack+yaml;v=1",
            "payload": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, pack_yaml),
            "signatures": [{
                "keyid": "sha256:commercial-key",
                "sig": "dGVzdCBzaWduYXR1cmU="
            }]
        });

        Mock::given(method("GET"))
            .and(path("/packs/commercial-pack/1.0.0.sig"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&envelope))
            .expect(1) // MUST be called
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let result = client
            .fetch_pack_with_signature("commercial-pack", "1.0.0", None)
            .await
            .expect("fetch failed");

        let (fetch, sig) = result.expect("expected Some");

        // Verify pack fetched
        assert_eq!(fetch.content, pack_yaml);

        // Verify signature was fetched from sidecar (not header)
        assert!(
            fetch.headers.signature.is_none(),
            "header signature should be absent"
        );
        assert!(sig.is_some(), "sidecar signature MUST be present");
        assert_eq!(sig.unwrap().signatures[0].key_id, "sha256:commercial-key");
    }

    #[tokio::test]
    async fn test_pack_304_signature_still_valid() {
        // Scenario: Pack returns 304 Not Modified
        // Question: Does signature need refetch?
        // Policy: No - if pack unchanged, signature unchanged (same digest binding)
        let mock_server = MockServer::start().await;

        // First fetch: get pack + signature
        let pack_yaml = "name: cached-pack\nversion: \"1.0.0\"";
        let _expected_digest = compute_digest(pack_yaml); // Unused in 304 test

        Mock::given(method("GET"))
            .and(path("/packs/cached-pack/1.0.0"))
            .and(header("if-none-match", "\"etag-abc\""))
            .respond_with(ResponseTemplate::new(304)) // Not Modified
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;

        // When pack is 304, fetch_pack returns None
        let result = client
            .fetch_pack("cached-pack", "1.0.0", Some("\"etag-abc\""))
            .await
            .expect("fetch failed");

        // 304 means: use cached pack AND cached signature
        // No need to refetch signature - digest binding unchanged
        assert!(
            result.is_none(),
            "304 should return None - use cached pack+signature"
        );

        // Document the policy: signature cache is tied to pack ETag/digest
        // If pack unchanged (304), signature is also unchanged
    }

    // ==================== Protocol Correctness Tests (SPEC §4) ====================

    #[tokio::test]
    async fn test_etag_is_strong_etag_format() {
        // SPEC §4.3: ETag MUST be strong ETag (quoted string)
        let mock_server = MockServer::start().await;

        let pack_yaml = "name: test\nversion: \"1.0.0\"";
        let digest = compute_digest(pack_yaml);
        // Strong ETag format: "value" (quoted)
        let etag = format!("\"{}\"", digest);

        Mock::given(method("GET"))
            .and(path("/packs/test/1.0.0"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(pack_yaml)
                    .insert_header("etag", etag.as_str())
                    .insert_header("x-pack-digest", digest.as_str()),
            )
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let result = client.fetch_pack("test", "1.0.0", None).await.unwrap();
        let fetch = result.unwrap();

        // ETag should be the quoted digest
        assert_eq!(fetch.headers.etag, Some(etag));
        // And should match X-Pack-Digest when unquoted
        let etag_unquoted = fetch.headers.etag.unwrap().trim_matches('"').to_string();
        assert_eq!(etag_unquoted, digest);
    }

    #[tokio::test]
    async fn test_vary_header_for_authenticated_response() {
        // SPEC §4.3: Vary: Authorization, Accept-Encoding for authenticated responses
        let mock_server = MockServer::start().await;

        let pack_yaml = "name: test\nversion: \"1.0.0\"";

        Mock::given(method("GET"))
            .and(path("/packs/test/1.0.0"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(pack_yaml)
                    .insert_header("vary", "Authorization, Accept-Encoding")
                    .insert_header("cache-control", "private, max-age=86400"),
            )
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let result = client.fetch_pack("test", "1.0.0", None).await;

        // Should succeed - we're just verifying the server can set Vary header
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_content_digest_vs_canonical_digest() {
        // SPEC §4.3: X-Pack-Digest is canonical (JCS), Content-Digest may differ
        // Wire bytes may have different whitespace but X-Pack-Digest is source of truth
        let mock_server = MockServer::start().await;

        // Wire content with extra whitespace
        let wire_content = "name:   test\nversion:    \"1.0.0\"\n\n";
        // Canonical content (what JCS produces)
        let canonical_content = "name: test\nversion: \"1.0.0\"";
        let canonical_digest = compute_digest(canonical_content);

        Mock::given(method("GET"))
            .and(path("/packs/test/1.0.0"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(wire_content)
                    // X-Pack-Digest is the canonical digest
                    .insert_header("x-pack-digest", canonical_digest.as_str()),
            )
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let result = client.fetch_pack("test", "1.0.0", None).await.unwrap();
        let fetch = result.unwrap();

        // Content is wire bytes
        assert_eq!(fetch.content, wire_content);
        // X-Pack-Digest header is canonical
        assert_eq!(fetch.headers.digest, Some(canonical_digest.clone()));
        // Computed digest should match canonical (JCS normalization)
        assert_eq!(fetch.computed_digest, canonical_digest);
    }

    #[tokio::test]
    async fn test_304_cache_hit_flow() {
        // SPEC §7.3: 304 response → use cached pack
        // This test verifies the client correctly handles 304 Not Modified
        let mock_server = MockServer::start().await;

        // When client sends If-None-Match with valid ETag, server returns 304
        Mock::given(method("GET"))
            .and(path("/packs/cached-pack/1.0.0"))
            .and(header("if-none-match", "\"sha256:abc123\""))
            .respond_with(ResponseTemplate::new(304))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;

        // Fetch with valid ETag - should get None (use cached)
        let result = client
            .fetch_pack("cached-pack", "1.0.0", Some("\"sha256:abc123\""))
            .await
            .unwrap();

        assert!(result.is_none(), "304 should return None - use cached pack");
    }

    #[tokio::test]
    async fn test_retry_on_429_with_retry_after() {
        // SPEC §4.4: 429 triggers retry with Retry-After header
        let mock_server = MockServer::start().await;

        // All requests return 429 - test that retry happens and eventually fails
        Mock::given(method("GET"))
            .and(path("/packs/retry-test/1.0.0"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "1"))
            .expect(2) // Initial + 1 retry (with max_retries=1)
            .mount(&mock_server)
            .await;

        let config = RegistryConfig {
            url: mock_server.uri(),
            token: Some("test-token".to_string()),
            max_retries: 1, // 1 retry = 2 total attempts
            timeout_secs: 30,
            ..Default::default()
        };
        let client = RegistryClient::new(config).unwrap();

        let start = std::time::Instant::now();
        let result = client.fetch_pack("retry-test", "1.0.0", None).await;
        let elapsed = start.elapsed();

        // Should fail after max retries
        assert!(
            matches!(result, Err(RegistryError::RateLimited { .. })),
            "Should fail with RateLimited"
        );

        // With retry-after: 1 and jitter, we expect at least ~850ms of backoff
        assert!(
            elapsed.as_millis() >= 850,
            "Should have waited for retry-after (with jitter), elapsed: {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_max_retries_exceeded() {
        // SPEC §4.4: Stop retrying after max_retries
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/packs/fail-test/1.0.0"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "1"))
            .expect(2) // Initial + 1 retry
            .mount(&mock_server)
            .await;

        let config = RegistryConfig {
            url: mock_server.uri(),
            token: Some("test-token".to_string()),
            max_retries: 1, // Only 1 retry
            timeout_secs: 30,
            ..Default::default()
        };
        let client = RegistryClient::new(config).unwrap();

        let result = client.fetch_pack("fail-test", "1.0.0", None).await;
        assert!(
            matches!(result, Err(RegistryError::RateLimited { .. })),
            "Should fail with RateLimited after max retries"
        );
    }
}
