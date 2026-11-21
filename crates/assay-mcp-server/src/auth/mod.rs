#![allow(deprecated)]

/// Legacy module path. Its configuration types were removed in 7.0.
pub mod config {}
pub mod jwks;
pub mod sensitive_headers;
pub mod validation;

pub use jwks::JwksProvider;
pub use sensitive_headers::{
    build_downstream_headers, is_sensitive, strip_sensitive_headers, SENSITIVE_HEADER_NAMES,
};
pub use validation::Claims;

#[cfg(test)]
pub mod tests;
