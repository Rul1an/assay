use super::super::OidcProvider;

pub(super) fn github_oidc_request_url(provider: &OidcProvider) -> String {
    format!(
        "{}&audience={}",
        provider.token_request_url, provider.audience
    )
}

pub(super) fn bearer(token: &str) -> String {
    format!("Bearer {}", token)
}

pub(super) fn accept_header() -> &'static str {
    "application/json; api-version=2.0"
}

pub(super) fn content_type_header() -> &'static str {
    "application/json"
}
