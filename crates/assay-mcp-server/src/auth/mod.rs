pub mod config;
pub mod jwks;
pub mod sensitive_headers;
pub mod validation;

pub use config::{AuthConfig, AuthMode};
pub use jwks::JwksProvider;
pub use sensitive_headers::{
    build_downstream_headers, is_sensitive, strip_sensitive_headers, SENSITIVE_HEADER_NAMES,
};
pub use validation::{Claims, TokenValidator};

#[cfg(test)]
pub mod tests;
