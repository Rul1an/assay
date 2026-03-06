//! Integration tests for RegistryClient.
//!
//! Uses wiremock for HTTP mocking. Tests cover fetch_pack, fetch_signature,
//! fetch_pack_with_signature, status mapping (304/404/404-sig/410/429/5xx), and retry behavior.

mod scenarios_auth_headers;
mod scenarios_cache_digest;
mod scenarios_meta_keys;
mod scenarios_pack_fetch;
mod scenarios_retry;
mod scenarios_signature;
mod support;

pub(super) use std::time::Duration;

pub(super) use assay_registry::{
    compute_digest, RegistryClient, RegistryConfig, RegistryError, REGISTRY_USER_AGENT,
};
pub(super) use wiremock::matchers::{header, method, path};
pub(super) use wiremock::{Mock, MockServer, ResponseTemplate};
