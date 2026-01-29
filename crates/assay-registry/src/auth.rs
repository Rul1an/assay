//! Token authentication for the registry.
//!
//! Supports multiple authentication methods:
//! - Static token (from config or env)
//! - OIDC token exchange (for CI environments like GitHub Actions)
//!
//! # GitHub Actions OIDC
//!
//! To use OIDC authentication in GitHub Actions:
//!
//! ```yaml
//! jobs:
//!   lint:
//!     runs-on: ubuntu-latest
//!     permissions:
//!       id-token: write  # Required for OIDC
//!       contents: read
//!     steps:
//!       - uses: actions/checkout@v4
//!       - name: Run assay lint
//!         run: assay evidence lint --pack eu-ai-act-pro@1.2.0 bundle.tar.gz
//!         env:
//!           ASSAY_REGISTRY_OIDC: "1"
//! ```

use crate::error::RegistryResult;

#[cfg(feature = "oidc")]
use crate::error::RegistryError;

/// Token provider for registry authentication.
#[derive(Debug, Clone)]
pub enum TokenProvider {
    /// Static token (from config or env).
    Static(String),

    /// No authentication.
    None,

    /// OIDC token exchange for CI environments.
    #[cfg(feature = "oidc")]
    Oidc(OidcProvider),
}

impl TokenProvider {
    /// Create a static token provider.
    pub fn static_token(token: impl Into<String>) -> Self {
        Self::Static(token.into())
    }

    /// Create from environment variable.
    ///
    /// Checks in order:
    /// 1. `ASSAY_REGISTRY_TOKEN` - static token
    /// 2. `ASSAY_REGISTRY_OIDC=1` + GitHub Actions env - OIDC exchange
    /// 3. Falls back to no auth
    pub fn from_env() -> Self {
        // Check for static token
        if let Ok(token) = std::env::var("ASSAY_REGISTRY_TOKEN") {
            if !token.is_empty() {
                return Self::Static(token);
            }
        }

        // Check for OIDC (with feature)
        #[cfg(feature = "oidc")]
        if std::env::var("ASSAY_REGISTRY_OIDC")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
        {
            if let Ok(provider) = OidcProvider::from_github_actions() {
                return Self::Oidc(provider);
            }
        }

        Self::None
    }

    /// Get the current token.
    ///
    /// For static tokens, returns the token directly.
    /// For OIDC, may perform token exchange if expired.
    pub async fn get_token(&self) -> RegistryResult<Option<String>> {
        match self {
            Self::Static(token) => Ok(Some(token.clone())),
            Self::None => Ok(None),
            #[cfg(feature = "oidc")]
            Self::Oidc(provider) => provider.get_token().await,
        }
    }

    /// Check if authentication is configured.
    pub fn is_authenticated(&self) -> bool {
        !matches!(self, Self::None)
    }

    /// Create an OIDC provider for GitHub Actions.
    #[cfg(feature = "oidc")]
    pub fn github_oidc() -> RegistryResult<Self> {
        Ok(Self::Oidc(OidcProvider::from_github_actions()?))
    }
}

impl Default for TokenProvider {
    fn default() -> Self {
        Self::from_env()
    }
}

/// OIDC token provider for CI environments.
///
/// Supports GitHub Actions OIDC tokens, exchanged for registry access tokens.
#[cfg(feature = "oidc")]
#[derive(Debug, Clone)]
pub struct OidcProvider {
    /// GitHub Actions OIDC token request URL.
    token_request_url: String,

    /// GitHub Actions request token (for authenticating to GitHub).
    request_token: String,

    /// Registry token exchange endpoint.
    registry_exchange_url: String,

    /// Registry audience.
    audience: String,

    /// Cached registry token.
    cached_token: std::sync::Arc<tokio::sync::RwLock<Option<CachedToken>>>,
}

#[cfg(feature = "oidc")]
#[derive(Debug, Clone)]
struct CachedToken {
    token: String,
    expires_at: chrono::DateTime<chrono::Utc>,
}

/// OIDC token exchange response.
#[cfg(feature = "oidc")]
#[derive(Debug, serde::Deserialize)]
struct OidcTokenResponse {
    value: String,
}

/// Registry token exchange response.
#[cfg(feature = "oidc")]
#[derive(Debug, serde::Deserialize)]
struct RegistryTokenResponse {
    access_token: String,
    expires_in: u64,
    token_type: String,
}

#[cfg(feature = "oidc")]
impl OidcProvider {
    /// Create from GitHub Actions environment.
    ///
    /// Requires:
    /// - `ACTIONS_ID_TOKEN_REQUEST_URL`: URL to request OIDC token
    /// - `ACTIONS_ID_TOKEN_REQUEST_TOKEN`: Token to authenticate request
    ///
    /// Optional:
    /// - `ASSAY_REGISTRY_URL`: Custom registry URL (default: https://registry.getassay.dev/v1)
    pub fn from_github_actions() -> RegistryResult<Self> {
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

        Ok(Self {
            token_request_url,
            request_token,
            registry_exchange_url,
            audience: "https://registry.getassay.dev".to_string(),
            cached_token: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        })
    }

    /// Create with custom URLs (for testing).
    pub fn new(
        token_request_url: impl Into<String>,
        request_token: impl Into<String>,
        registry_exchange_url: impl Into<String>,
        audience: impl Into<String>,
    ) -> Self {
        Self {
            token_request_url: token_request_url.into(),
            request_token: request_token.into(),
            registry_exchange_url: registry_exchange_url.into(),
            audience: audience.into(),
            cached_token: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        }
    }

    /// Get token, refreshing if expired.
    pub async fn get_token(&self) -> RegistryResult<Option<String>> {
        // Check cache first
        {
            let cache = self.cached_token.read().await;
            if let Some(cached) = cache.as_ref() {
                // Use 60-second buffer before expiry + 30s clock skew tolerance
                let buffer = chrono::Duration::seconds(90);
                if cached.expires_at > chrono::Utc::now() + buffer {
                    tracing::debug!("using cached OIDC token");
                    return Ok(Some(cached.token.clone()));
                }
            }
        }

        // Need to refresh
        tracing::debug!("refreshing OIDC token");
        let token = self.exchange_token_with_retry().await?;
        Ok(Some(token))
    }

    /// Exchange token with exponential backoff retry.
    async fn exchange_token_with_retry(&self) -> RegistryResult<String> {
        let mut retries = 0;
        let max_retries = 3;

        loop {
            match self.exchange_token().await {
                Ok(token) => return Ok(token),
                Err(e) if retries < max_retries => {
                    retries += 1;

                    // Exponential backoff: 1s, 2s, 4s, capped at 30s
                    let backoff = std::time::Duration::from_secs(1 << retries);
                    let backoff = backoff.min(std::time::Duration::from_secs(30));

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

    /// Exchange OIDC token for registry token.
    async fn exchange_token(&self) -> RegistryResult<String> {
        // 1. Get OIDC token from GitHub
        let oidc_token = self.get_github_oidc_token().await?;

        // 2. Exchange for registry token
        let registry_token = self.exchange_for_registry_token(&oidc_token).await?;

        Ok(registry_token)
    }

    /// Request OIDC token from GitHub Actions.
    async fn get_github_oidc_token(&self) -> RegistryResult<String> {
        let client = reqwest::Client::new();

        let url = format!("{}&audience={}", self.token_request_url, self.audience);

        let response = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.request_token))
            .header("Accept", "application/json; api-version=2.0")
            .header("Content-Type", "application/json")
            .send()
            .await
            .map_err(|e| RegistryError::Network {
                message: format!("failed to request GitHub OIDC token: {}", e),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(RegistryError::Unauthorized {
                message: format!("GitHub OIDC request failed: HTTP {} - {}", status, body),
            });
        }

        let token_response: OidcTokenResponse =
            response
                .json()
                .await
                .map_err(|e| RegistryError::InvalidResponse {
                    message: format!("failed to parse GitHub OIDC response: {}", e),
                })?;

        Ok(token_response.value)
    }

    /// Exchange GitHub OIDC token for registry access token.
    async fn exchange_for_registry_token(&self, oidc_token: &str) -> RegistryResult<String> {
        let client = reqwest::Client::new();

        let response = client
            .post(&self.registry_exchange_url)
            .json(&serde_json::json!({
                "token": oidc_token,
                "grant_type": "urn:ietf:params:oauth:grant-type:token-exchange",
                "subject_token_type": "urn:ietf:params:oauth:token-type:jwt"
            }))
            .send()
            .await
            .map_err(|e| RegistryError::Network {
                message: format!("failed to exchange token: {}", e),
            })?;

        let status = response.status();

        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(RegistryError::Unauthorized {
                message: "OIDC token exchange failed: unauthorized".to_string(),
            });
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(RegistryError::Network {
                message: format!("token exchange failed: HTTP {} - {}", status, body),
            });
        }

        let token_response: RegistryTokenResponse =
            response
                .json()
                .await
                .map_err(|e| RegistryError::InvalidResponse {
                    message: format!("failed to parse registry token response: {}", e),
                })?;

        // Cache the token
        let expires_at =
            chrono::Utc::now() + chrono::Duration::seconds(token_response.expires_in as i64);

        {
            let mut cache = self.cached_token.write().await;
            *cache = Some(CachedToken {
                token: token_response.access_token.clone(),
                expires_at,
            });
        }

        tracing::info!(
            expires_in = token_response.expires_in,
            token_type = %token_response.token_type,
            "obtained registry access token"
        );

        Ok(token_response.access_token)
    }

    /// Clear the cached token.
    pub async fn clear_cache(&self) {
        let mut cache = self.cached_token.write().await;
        *cache = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_static_token() {
        let provider = TokenProvider::static_token("test-token");
        assert!(provider.is_authenticated());
    }

    #[test]
    fn test_no_auth() {
        let provider = TokenProvider::None;
        assert!(!provider.is_authenticated());
    }

    #[tokio::test]
    async fn test_get_static_token() {
        let provider = TokenProvider::static_token("my-token");
        let token = provider.get_token().await.unwrap();
        assert_eq!(token, Some("my-token".to_string()));
    }

    #[tokio::test]
    async fn test_get_no_token() {
        let provider = TokenProvider::None;
        let token = provider.get_token().await.unwrap();
        assert_eq!(token, None);
    }

    #[test]
    #[serial]
    fn test_from_env_static() {
        // Clean environment first
        std::env::remove_var("ASSAY_REGISTRY_TOKEN");
        std::env::remove_var("ASSAY_REGISTRY_OIDC");

        std::env::set_var("ASSAY_REGISTRY_TOKEN", "env-token");
        let provider = TokenProvider::from_env();
        std::env::remove_var("ASSAY_REGISTRY_TOKEN");

        assert!(matches!(provider, TokenProvider::Static(_)));
    }

    #[test]
    #[serial]
    fn test_from_env_empty_token() {
        // Clean environment first
        std::env::remove_var("ASSAY_REGISTRY_TOKEN");
        std::env::remove_var("ASSAY_REGISTRY_OIDC");

        std::env::set_var("ASSAY_REGISTRY_TOKEN", "");
        let provider = TokenProvider::from_env();
        std::env::remove_var("ASSAY_REGISTRY_TOKEN");

        assert!(matches!(provider, TokenProvider::None));
    }
}

#[cfg(all(test, feature = "oidc"))]
mod oidc_tests {
    use super::*;
    use wiremock::matchers::{body_json, header, method, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_oidc_full_flow() {
        let github_mock = MockServer::start().await;
        let registry_mock = MockServer::start().await;

        // Mock GitHub OIDC token endpoint
        Mock::given(method("GET"))
            .and(query_param("audience", "https://registry.test"))
            .and(header("authorization", "Bearer gh-request-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "value": "github-oidc-jwt-token"
            })))
            .mount(&github_mock)
            .await;

        // Mock registry token exchange endpoint
        Mock::given(method("POST"))
            .and(body_json(serde_json::json!({
                "token": "github-oidc-jwt-token",
                "grant_type": "urn:ietf:params:oauth:grant-type:token-exchange",
                "subject_token_type": "urn:ietf:params:oauth:token-type:jwt"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "registry-access-token",
                "expires_in": 3600,
                "token_type": "Bearer"
            })))
            .mount(&registry_mock)
            .await;

        let provider = OidcProvider::new(
            format!("{}?foo=bar", github_mock.uri()),
            "gh-request-token",
            format!("{}/auth/oidc/exchange", registry_mock.uri()),
            "https://registry.test",
        );

        let token = provider.get_token().await.unwrap();
        assert_eq!(token, Some("registry-access-token".to_string()));

        // Second call should use cache
        let token2 = provider.get_token().await.unwrap();
        assert_eq!(token2, Some("registry-access-token".to_string()));
    }

    #[tokio::test]
    async fn test_oidc_github_failure() {
        let github_mock = MockServer::start().await;

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(&github_mock)
            .await;

        let provider = OidcProvider::new(
            format!("{}?foo=bar", github_mock.uri()),
            "bad-token",
            "https://registry.test/auth/oidc/exchange",
            "https://registry.test",
        );

        let result = provider.get_token().await;
        assert!(matches!(result, Err(RegistryError::Unauthorized { .. })));
    }

    #[tokio::test]
    async fn test_oidc_cache_clear() {
        let provider = OidcProvider::new(
            "https://github.example/token?foo=bar",
            "token",
            "https://registry.test/exchange",
            "https://registry.test",
        );

        // Set cache manually
        {
            let mut cache = provider.cached_token.write().await;
            *cache = Some(CachedToken {
                token: "cached-token".to_string(),
                expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            });
        }

        // Verify cache works
        let token = provider.get_token().await.unwrap();
        assert_eq!(token, Some("cached-token".to_string()));

        // Clear cache
        provider.clear_cache().await;

        // Cache should be empty now (will fail on network call)
        let cache = provider.cached_token.read().await;
        assert!(cache.is_none());
    }
}
