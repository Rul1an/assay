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

#[path = "auth_next/mod.rs"]
mod auth_next;

use crate::error::RegistryResult;

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
        auth_next::providers::static_token(token)
    }

    /// Create from environment variable.
    ///
    /// Checks in order:
    /// 1. `ASSAY_REGISTRY_TOKEN` - static token
    /// 2. `ASSAY_REGISTRY_OIDC=1` + GitHub Actions env - OIDC exchange
    /// 3. Falls back to no auth
    pub fn from_env() -> Self {
        auth_next::providers::from_env()
    }

    /// Get the current token.
    ///
    /// For static tokens, returns the token directly.
    /// For OIDC, may perform token exchange if expired.
    pub async fn get_token(&self) -> RegistryResult<Option<String>> {
        auth_next::providers::get_token(self).await
    }

    /// Check if authentication is configured.
    pub fn is_authenticated(&self) -> bool {
        auth_next::providers::is_authenticated(self)
    }

    /// Create an OIDC provider for GitHub Actions.
    #[cfg(feature = "oidc")]
    pub fn github_oidc() -> RegistryResult<Self> {
        auth_next::providers::github_oidc()
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
        auth_next::oidc::from_github_actions()
    }

    /// Create with custom URLs (for testing).
    pub fn new(
        token_request_url: impl Into<String>,
        request_token: impl Into<String>,
        registry_exchange_url: impl Into<String>,
        audience: impl Into<String>,
    ) -> Self {
        auth_next::oidc::new(
            token_request_url,
            request_token,
            registry_exchange_url,
            audience,
        )
    }

    /// Get token, refreshing if expired.
    pub async fn get_token(&self) -> RegistryResult<Option<String>> {
        auth_next::cache::get_token(self).await
    }

    /// Exchange token with exponential backoff retry.
    async fn exchange_token_with_retry(&self) -> RegistryResult<String> {
        auth_next::oidc::exchange_token_with_retry(self).await
    }

    /// Exchange OIDC token for registry token.
    async fn exchange_token(&self) -> RegistryResult<String> {
        auth_next::oidc::exchange_token(self).await
    }

    /// Request OIDC token from GitHub Actions.
    async fn get_github_oidc_token(&self) -> RegistryResult<String> {
        auth_next::oidc::get_github_oidc_token(self).await
    }

    /// Exchange GitHub OIDC token for registry access token.
    async fn exchange_for_registry_token(&self, oidc_token: &str) -> RegistryResult<String> {
        auth_next::oidc::exchange_for_registry_token(self, oidc_token).await
    }

    /// Clear the cached token.
    pub async fn clear_cache(&self) {
        auth_next::cache::clear_cache(self).await
    }
}
