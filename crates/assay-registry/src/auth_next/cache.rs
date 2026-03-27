use crate::error::RegistryResult;

use super::super::{CachedToken, OidcProvider};

pub(in crate::auth) async fn get_token(provider: &OidcProvider) -> RegistryResult<Option<String>> {
    {
        let cache = provider.cached_token.read().await;
        if let Some(cached) = cache.as_ref() {
            let buffer = chrono::Duration::seconds(90);
            if cached.expires_at > chrono::Utc::now() + buffer {
                tracing::debug!("using cached OIDC token");
                return Ok(Some(cached.token.clone()));
            }
        }
    }

    tracing::debug!("refreshing OIDC token");
    let token = provider.exchange_token_with_retry().await?;
    Ok(Some(token))
}

pub(in crate::auth) async fn cache_token(provider: &OidcProvider, token: String, expires_in: u64) {
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(expires_in as i64);

    let mut cache = provider.cached_token.write().await;
    *cache = Some(CachedToken { token, expires_at });
}

pub(in crate::auth) async fn clear_cache(provider: &OidcProvider) {
    let mut cache = provider.cached_token.write().await;
    *cache = None;
}
