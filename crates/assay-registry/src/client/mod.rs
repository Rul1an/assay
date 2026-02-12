//! Registry client for fetching packs.
//!
//! Public API: no status code knowledge. All HTTP/status mapping in http.rs.

use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use tracing::debug;

use crate::auth::TokenProvider;
use crate::error::{RegistryError, RegistryResult};
use crate::types::{
    DsseEnvelope, FetchResult, KeysManifest, PackHeaders, PackMeta, RegistryConfig,
    VersionsResponse,
};

mod helpers;
mod http;

use helpers::compute_digest;
use http::{HttpBackend, PackOutcome, SignatureOutcome};

const USER_AGENT_VALUE: &str = concat!("assay-registry/", env!("CARGO_PKG_VERSION"));

/// Registry client for fetching packs.
#[derive(Debug, Clone)]
pub struct RegistryClient {
    http: HttpBackend,
}

impl RegistryClient {
    pub fn new(config: RegistryConfig) -> RegistryResult<Self> {
        let token_provider = config
            .token
            .as_ref()
            .map(TokenProvider::static_token)
            .unwrap_or_else(TokenProvider::from_env);

        Self::with_token_provider(config, token_provider)
    }

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

        let base_url = config.url.trim_end_matches('/').to_string();

        Ok(Self {
            http: HttpBackend {
                client,
                base_url: base_url.clone(),
                token_provider,
                config,
            },
        })
    }

    pub fn from_env() -> RegistryResult<Self> {
        Self::new(RegistryConfig::from_env())
    }

    pub async fn get_pack_meta(&self, name: &str, version: &str) -> RegistryResult<PackMeta> {
        let url = self.pack_url(name, version);
        debug!(url = %url, "fetching pack metadata");

        let response = self.http.request(reqwest::Method::HEAD, &url, None).await?;
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

    pub async fn fetch_pack(
        &self,
        name: &str,
        version: &str,
        etag: Option<&str>,
    ) -> RegistryResult<Option<FetchResult>> {
        let url = self.pack_url(name, version);
        debug!(url = %url, etag = ?etag, "fetching pack content");

        match self.http.fetch_pack(&url, etag).await? {
            PackOutcome::NotModified => Ok(None),
            PackOutcome::Fetched(f) => {
                let computed_digest = compute_digest(&f.content);
                Ok(Some(FetchResult {
                    content: f.content,
                    headers: f.headers,
                    computed_digest,
                }))
            }
        }
    }

    pub async fn list_versions(&self, name: &str) -> RegistryResult<VersionsResponse> {
        let url = format!("{}/packs/{}/versions", self.http.base_url, name);
        debug!(url = %url, "listing pack versions");

        let response = self.http.request(reqwest::Method::GET, &url, None).await?;

        response
            .json()
            .await
            .map_err(|e| RegistryError::InvalidResponse {
                message: format!("failed to parse versions response: {}", e),
            })
    }

    pub async fn fetch_keys(&self) -> RegistryResult<KeysManifest> {
        let url = format!("{}/keys", self.http.base_url);
        debug!(url = %url, "fetching keys manifest");

        let response = self.http.request(reqwest::Method::GET, &url, None).await?;

        response
            .json()
            .await
            .map_err(|e| RegistryError::InvalidResponse {
                message: format!("failed to parse keys manifest: {}", e),
            })
    }

    pub async fn fetch_signature(
        &self,
        name: &str,
        version: &str,
    ) -> RegistryResult<Option<DsseEnvelope>> {
        let url = self.signature_url(name, version);
        debug!(url = %url, "fetching signature from sidecar");

        match self.http.fetch_signature_optional(&url).await? {
            SignatureOutcome::Missing => Ok(None),
            SignatureOutcome::Present(text) => {
                let envelope: DsseEnvelope =
                    serde_json::from_str(&text).map_err(|e| RegistryError::InvalidResponse {
                        message: format!("failed to parse signature envelope: {}", e),
                    })?;
                Ok(Some(envelope))
            }
        }
    }

    pub async fn fetch_pack_with_signature(
        &self,
        name: &str,
        version: &str,
        etag: Option<&str>,
    ) -> RegistryResult<Option<(FetchResult, Option<DsseEnvelope>)>> {
        let pack_result = self.fetch_pack(name, version, etag).await?;

        let fetch = match pack_result {
            Some(f) => f,
            None => return Ok(None),
        };

        let signature = self.fetch_signature(name, version).await?;

        Ok(Some((fetch, signature)))
    }

    fn pack_url(&self, name: &str, version: &str) -> String {
        format!("{}/packs/{}/{}", self.http.base_url, name, version)
    }

    fn signature_url(&self, name: &str, version: &str) -> String {
        format!("{}/packs/{}/{}.sig", self.http.base_url, name, version)
    }

    pub fn base_url(&self) -> &str {
        &self.http.base_url
    }

    pub fn is_authenticated(&self) -> bool {
        self.http.token_provider.is_authenticated()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonicalize::compute_canonical_digest;
    use crate::digest::sha256_hex_bytes;
    use crate::verify::compute_digest as verify_compute_digest;

    #[test]
    fn test_compute_digest_canonical() {
        let content = "name: test\nversion: \"1.0.0\"";
        let digest = compute_digest(content);
        assert!(digest.starts_with("sha256:"));
        assert_eq!(digest.len(), 7 + 64);

        let expected = compute_canonical_digest(content).unwrap();
        assert_eq!(digest, expected);
    }

    #[test]
    fn test_compute_digest_non_yaml_fallback() {
        let content = "this is not: valid: yaml: [[";
        let digest = compute_digest(content);
        assert!(digest.starts_with("sha256:"));
        assert_eq!(digest.len(), 7 + 64);
    }

    #[test]
    fn test_compute_digest_parity_with_verify_module() {
        let canonical_yaml = "name: test\nversion: \"1.0.0\"\nkind: compliance";
        let invalid_yaml = "this is not: valid: yaml: [[";

        assert!(compute_canonical_digest(canonical_yaml).is_ok());
        assert_eq!(
            compute_digest(canonical_yaml),
            verify_compute_digest(canonical_yaml)
        );

        assert!(compute_canonical_digest(invalid_yaml).is_err());
        let raw_fallback = sha256_hex_bytes(invalid_yaml.as_bytes());
        assert_eq!(compute_digest(invalid_yaml), raw_fallback);
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
        let (name, version) = helpers::parse_pack_url(url);
        assert_eq!(name, "eu-ai-act");
        assert_eq!(version, "1.2.0");
    }

    #[test]
    fn test_config_from_env_defaults() {
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
