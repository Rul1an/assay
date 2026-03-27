use crate::error::RegistryResult;

#[cfg(feature = "oidc")]
use super::super::OidcProvider;
use super::super::TokenProvider;

pub(in crate::auth) fn static_token(token: impl Into<String>) -> TokenProvider {
    TokenProvider::Static(token.into())
}

pub(in crate::auth) fn from_env() -> TokenProvider {
    if let Ok(token) = std::env::var("ASSAY_REGISTRY_TOKEN") {
        if !token.is_empty() {
            return TokenProvider::Static(token);
        }
    }

    #[cfg(feature = "oidc")]
    if std::env::var("ASSAY_REGISTRY_OIDC")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        if let Ok(provider) = OidcProvider::from_github_actions() {
            return TokenProvider::Oidc(provider);
        }
    }

    TokenProvider::None
}

pub(in crate::auth) async fn get_token(provider: &TokenProvider) -> RegistryResult<Option<String>> {
    match provider {
        TokenProvider::Static(token) => Ok(Some(token.clone())),
        TokenProvider::None => Ok(None),
        #[cfg(feature = "oidc")]
        TokenProvider::Oidc(provider) => provider.get_token().await,
    }
}

pub(in crate::auth) fn is_authenticated(provider: &TokenProvider) -> bool {
    !matches!(provider, TokenProvider::None)
}

#[cfg(feature = "oidc")]
pub(in crate::auth) fn github_oidc() -> RegistryResult<TokenProvider> {
    Ok(TokenProvider::Oidc(OidcProvider::from_github_actions()?))
}
