use crate::error::{RegistryError, RegistryResult};

use super::super::OidcProvider;
use super::{cache, diagnostics, headers};

#[derive(Debug, serde::Deserialize)]
struct OidcTokenResponse {
    value: String,
}

#[derive(Debug, serde::Deserialize)]
struct RegistryTokenResponse {
    access_token: String,
    expires_in: u64,
    token_type: String,
}

pub(in crate::auth) fn from_github_actions() -> RegistryResult<OidcProvider> {
    let token_request_url = std::env::var("ACTIONS_ID_TOKEN_REQUEST_URL").map_err(|_| {
        RegistryError::Config {
            message: "ACTIONS_ID_TOKEN_REQUEST_URL not set - not in GitHub Actions or id-token permission not granted".into(),
        }
    })?;

    let request_token =
        std::env::var("ACTIONS_ID_TOKEN_REQUEST_TOKEN").map_err(|_| RegistryError::Config {
            message: "ACTIONS_ID_TOKEN_REQUEST_TOKEN not set".into(),
        })?;

    let registry_base = std::env::var("ASSAY_REGISTRY_URL")
        .unwrap_or_else(|_| "https://registry.getassay.dev/v1".to_string());
    let registry_exchange_url =
        format!("{}/auth/oidc/exchange", registry_base.trim_end_matches('/'));

    Ok(OidcProvider {
        token_request_url,
        request_token,
        registry_exchange_url,
        audience: "https://registry.getassay.dev".to_string(),
        cached_token: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
    })
}

pub(in crate::auth) fn new(
    token_request_url: impl Into<String>,
    request_token: impl Into<String>,
    registry_exchange_url: impl Into<String>,
    audience: impl Into<String>,
) -> OidcProvider {
    OidcProvider {
        token_request_url: token_request_url.into(),
        request_token: request_token.into(),
        registry_exchange_url: registry_exchange_url.into(),
        audience: audience.into(),
        cached_token: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
    }
}

pub(in crate::auth) async fn exchange_token_with_retry(
    provider: &OidcProvider,
) -> RegistryResult<String> {
    let mut retries = 0;
    let max_retries = 3;

    loop {
        match provider.exchange_token().await {
            Ok(token) => return Ok(token),
            Err(e) if retries < max_retries => {
                retries += 1;

                let backoff = std::time::Duration::from_secs(1 << retries)
                    .min(std::time::Duration::from_secs(30));

                tracing::warn!(
                    error = %e,
                    retry = retries,
                    backoff_secs = backoff.as_secs(),
                    "OIDC token exchange failed, retrying"
                );

                tokio::time::sleep(backoff).await;
            }
            Err(e) => return Err(e),
        }
    }
}

pub(in crate::auth) async fn exchange_token(provider: &OidcProvider) -> RegistryResult<String> {
    let oidc_token = provider.get_github_oidc_token().await?;
    provider.exchange_for_registry_token(&oidc_token).await
}

pub(in crate::auth) async fn get_github_oidc_token(
    provider: &OidcProvider,
) -> RegistryResult<String> {
    let client = reqwest::Client::new();
    let url = headers::github_oidc_request_url(provider);

    let response = client
        .get(&url)
        .header("Authorization", headers::bearer(&provider.request_token))
        .header("Accept", headers::accept_header())
        .header("Content-Type", headers::content_type_header())
        .send()
        .await
        .map_err(diagnostics::github_oidc_network_error)?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(diagnostics::github_oidc_request_failed(status, body));
    }

    let token_response: OidcTokenResponse = response
        .json()
        .await
        .map_err(diagnostics::github_oidc_parse_error)?;

    Ok(token_response.value)
}

pub(in crate::auth) async fn exchange_for_registry_token(
    provider: &OidcProvider,
    oidc_token: &str,
) -> RegistryResult<String> {
    let client = reqwest::Client::new();

    let response = client
        .post(&provider.registry_exchange_url)
        .json(&serde_json::json!({
            "token": oidc_token,
            "grant_type": "urn:ietf:params:oauth:grant-type:token-exchange",
            "subject_token_type": "urn:ietf:params:oauth:token-type:jwt"
        }))
        .send()
        .await
        .map_err(diagnostics::token_exchange_network_error)?;

    let status = response.status();

    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(diagnostics::token_exchange_unauthorized());
    }

    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(diagnostics::token_exchange_failed(status, body));
    }

    let token_response: RegistryTokenResponse = response
        .json()
        .await
        .map_err(diagnostics::token_exchange_parse_error)?;

    cache::cache_token(
        provider,
        token_response.access_token.clone(),
        token_response.expires_in,
    )
    .await;

    tracing::info!(
        expires_in = token_response.expires_in,
        token_type = %token_response.token_type,
        "obtained registry access token"
    );

    Ok(token_response.access_token)
}
