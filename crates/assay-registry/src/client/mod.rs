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

        let signature = self.fetch_signature(name, version).await.ok().flatten();

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

#[cfg(test)]
mod integration_tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn create_test_client(mock_server: &MockServer) -> RegistryClient {
        let config = RegistryConfig::default()
            .with_url(mock_server.uri())
            .with_token("test-token");
        RegistryClient::new(config).expect("failed to create client")
    }

    #[tokio::test]
    async fn test_fetch_pack_success() {
        let mock_server = MockServer::start().await;

        let pack_yaml = "name: test-pack\nversion: \"1.0.0\"\nrules: []";
        let expected_digest = compute_digest(pack_yaml);

        Mock::given(method("GET"))
            .and(path("/packs/test-pack/1.0.0"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(pack_yaml)
                    .insert_header("x-pack-digest", expected_digest.as_str())
                    .insert_header("etag", "\"abc123\""),
            )
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let result = client
            .fetch_pack("test-pack", "1.0.0", None)
            .await
            .expect("fetch failed");

        let fetch = result.expect("expected Some");
        assert_eq!(fetch.content, pack_yaml);
        assert_eq!(fetch.computed_digest, expected_digest);
        assert_eq!(fetch.headers.etag, Some("\"abc123\"".to_string()));
    }

    #[tokio::test]
    async fn test_fetch_pack_304_not_modified() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/packs/test-pack/1.0.0"))
            .and(header("if-none-match", "\"abc123\""))
            .respond_with(ResponseTemplate::new(304))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let result = client
            .fetch_pack("test-pack", "1.0.0", Some("\"abc123\""))
            .await
            .expect("fetch failed");

        assert!(result.is_none(), "expected None for 304");
    }

    #[tokio::test]
    async fn test_fetch_pack_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/packs/nonexistent/1.0.0"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let result = client.fetch_pack("nonexistent", "1.0.0", None).await;

        assert!(matches!(result, Err(RegistryError::NotFound { .. })));
    }

    #[tokio::test]
    async fn test_fetch_pack_unauthorized() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/packs/private-pack/1.0.0"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let result = client.fetch_pack("private-pack", "1.0.0", None).await;

        assert!(matches!(result, Err(RegistryError::Unauthorized { .. })));
    }

    #[tokio::test]
    async fn test_fetch_pack_revoked_header_only() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/packs/revoked-pack/1.0.0"))
            .respond_with(
                ResponseTemplate::new(410)
                    .insert_header("x-revocation-reason", "security vulnerability"),
            )
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let result = client.fetch_pack("revoked-pack", "1.0.0", None).await;

        match result {
            Err(RegistryError::Revoked {
                name,
                version,
                reason,
                safe_version,
            }) => {
                assert_eq!(name, "revoked-pack");
                assert_eq!(version, "1.0.0");
                assert_eq!(reason, "security vulnerability");
                assert!(safe_version.is_none());
            }
            _ => panic!("expected Revoked error"),
        }
    }

    #[tokio::test]
    async fn test_fetch_pack_revoked_with_body() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/packs/revoked-pack/1.0.0"))
            .respond_with(ResponseTemplate::new(410).set_body_json(serde_json::json!({
                "reason": "critical CVE",
                "safe_version": "1.0.1"
            })))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let result = client.fetch_pack("revoked-pack", "1.0.0", None).await;

        match result {
            Err(RegistryError::Revoked {
                name,
                version,
                reason,
                safe_version,
            }) => {
                assert_eq!(name, "revoked-pack");
                assert_eq!(version, "1.0.0");
                assert_eq!(reason, "critical CVE");
                assert_eq!(safe_version, Some("1.0.1".to_string()));
            }
            _ => panic!("expected Revoked error with safe_version"),
        }
    }

    #[tokio::test]
    async fn test_rate_limiting_with_retry_after() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/packs/rate-limited/1.0.0"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "5"))
            .mount(&mock_server)
            .await;

        let config = RegistryConfig {
            url: mock_server.uri(),
            token: Some("test-token".to_string()),
            max_retries: 0,
            ..Default::default()
        };
        let client = RegistryClient::new(config).expect("failed to create client");
        let result = client.fetch_pack("rate-limited", "1.0.0", None).await;

        match result {
            Err(RegistryError::RateLimited { retry_after }) => {
                assert_eq!(retry_after, Some(Duration::from_secs(5)));
            }
            _ => panic!("expected RateLimited error"),
        }
    }

    #[tokio::test]
    async fn test_list_versions() {
        let mock_server = MockServer::start().await;

        let versions_json = serde_json::json!({
            "name": "test-pack",
            "versions": [
                {"version": "1.2.0", "digest": "sha256:abc123", "deprecated": false},
                {"version": "1.1.0", "digest": "sha256:def456", "deprecated": false},
                {"version": "1.0.0", "digest": "sha256:789abc", "deprecated": true}
            ]
        });

        Mock::given(method("GET"))
            .and(path("/packs/test-pack/versions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&versions_json))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let response = client
            .list_versions("test-pack")
            .await
            .expect("list versions failed");

        assert_eq!(response.name, "test-pack");
        assert_eq!(response.versions.len(), 3);
        assert_eq!(response.versions[0].version, "1.2.0");
        assert!(response.versions[2].deprecated);
    }

    #[tokio::test]
    async fn test_get_pack_meta() {
        let mock_server = MockServer::start().await;

        Mock::given(method("HEAD"))
            .and(path("/packs/test-pack/1.0.0"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("x-pack-digest", "sha256:abc123")
                    .insert_header("x-pack-signature", "dGVzdC1zaWduYXR1cmU=")
                    .insert_header("x-pack-key-id", "sha256:keyid123")
                    .insert_header("content-length", "1024"),
            )
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let meta = client
            .get_pack_meta("test-pack", "1.0.0")
            .await
            .expect("get meta failed");

        assert_eq!(meta.name, "test-pack");
        assert_eq!(meta.version, "1.0.0");
        assert_eq!(meta.digest, "sha256:abc123");
        assert!(meta.signed);
        assert_eq!(meta.key_id, Some("sha256:keyid123".to_string()));
        assert_eq!(meta.size, Some(1024));
    }

    #[tokio::test]
    async fn test_fetch_keys_manifest() {
        let mock_server = MockServer::start().await;

        let keys_json = serde_json::json!({
            "version": 1,
            "keys": [
                {
                    "key_id": "sha256:abc123",
                    "algorithm": "Ed25519",
                    "public_key": "dGVzdC1wdWJsaWMta2V5",
                    "description": "Production signing key"
                }
            ]
        });

        Mock::given(method("GET"))
            .and(path("/keys"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&keys_json))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let manifest = client.fetch_keys().await.expect("fetch keys failed");

        assert_eq!(manifest.version, 1);
        assert_eq!(manifest.keys.len(), 1);
        assert_eq!(manifest.keys[0].key_id, "sha256:abc123");
        assert_eq!(manifest.keys[0].algorithm, "Ed25519");
    }

    #[tokio::test]
    async fn test_authentication_header() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/packs/test/1.0.0"))
            .and(header("authorization", "Bearer secret-token"))
            .respond_with(ResponseTemplate::new(200).set_body_string("content"))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = RegistryConfig::default()
            .with_url(mock_server.uri())
            .with_token("secret-token");
        let client = RegistryClient::new(config).expect("failed to create client");

        let result = client.fetch_pack("test", "1.0.0", None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_no_auth_when_no_token() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/packs/public/1.0.0"))
            .respond_with(ResponseTemplate::new(200).set_body_string("content"))
            .mount(&mock_server)
            .await;

        let config = RegistryConfig {
            url: mock_server.uri(),
            token: None,
            ..Default::default()
        };
        let client = RegistryClient::new(config).expect("failed to create client");

        assert!(!client.is_authenticated());
        let result = client.fetch_pack("public", "1.0.0", None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_user_agent_header() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/packs/test/1.0.0"))
            .and(header("user-agent", USER_AGENT_VALUE))
            .respond_with(ResponseTemplate::new(200).set_body_string("content"))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let _ = client.fetch_pack("test", "1.0.0", None).await;
    }

    #[tokio::test]
    async fn test_fetch_signature_sidecar() {
        let mock_server = MockServer::start().await;

        let envelope = serde_json::json!({
            "payloadType": "application/vnd.assay.pack+yaml;v=1",
            "payload": "dGVzdCBwYXlsb2Fk",
            "signatures": [{
                "keyid": "sha256:abc123",
                "sig": "dGVzdCBzaWduYXR1cmU="
            }]
        });

        Mock::given(method("GET"))
            .and(path("/packs/signed-pack/1.0.0.sig"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&envelope))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let result = client
            .fetch_signature("signed-pack", "1.0.0")
            .await
            .expect("fetch signature failed");

        let sig = result.expect("expected Some");
        assert_eq!(sig.payload_type, "application/vnd.assay.pack+yaml;v=1");
        assert_eq!(sig.signatures.len(), 1);
        assert_eq!(sig.signatures[0].key_id, "sha256:abc123");
    }

    #[tokio::test]
    async fn test_fetch_signature_sidecar_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/packs/unsigned-pack/1.0.0.sig"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let result = client
            .fetch_signature("unsigned-pack", "1.0.0")
            .await
            .expect("fetch signature should not error on 404");

        assert!(result.is_none(), "expected None for unsigned pack");
    }

    #[tokio::test]
    async fn test_fetch_pack_with_signature() {
        let mock_server = MockServer::start().await;

        let pack_yaml = "name: signed-pack\nversion: \"1.0.0\"";
        let expected_digest = compute_digest(pack_yaml);

        Mock::given(method("GET"))
            .and(path("/packs/signed-pack/1.0.0"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(pack_yaml)
                    .insert_header("x-pack-digest", expected_digest.as_str()),
            )
            .mount(&mock_server)
            .await;

        let envelope = serde_json::json!({
            "payloadType": "application/vnd.assay.pack+yaml;v=1",
            "payload": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, pack_yaml),
            "signatures": [{
                "keyid": "sha256:key123",
                "sig": "dGVzdCBzaWduYXR1cmU="
            }]
        });

        Mock::given(method("GET"))
            .and(path("/packs/signed-pack/1.0.0.sig"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&envelope))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let result = client
            .fetch_pack_with_signature("signed-pack", "1.0.0", None)
            .await
            .expect("fetch failed");

        let (fetch, sig) = result.expect("expected Some");
        assert_eq!(fetch.content, pack_yaml);
        assert!(sig.is_some());
        assert_eq!(sig.unwrap().signatures[0].key_id, "sha256:key123");
    }

    #[tokio::test]
    async fn test_commercial_pack_signature_required_via_sidecar_only() {
        let mock_server = MockServer::start().await;

        let pack_yaml = "name: commercial-pack\nversion: \"1.0.0\"\nlicense: commercial";
        let expected_digest = compute_digest(pack_yaml);

        Mock::given(method("GET"))
            .and(path("/packs/commercial-pack/1.0.0"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(pack_yaml)
                    .insert_header("x-pack-digest", expected_digest.as_str())
                    .insert_header("x-pack-license", "LicenseRef-Assay-Enterprise-1.0")
                    .insert_header(
                        "x-pack-signature-endpoint",
                        "/packs/commercial-pack/1.0.0.sig",
                    ),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        let envelope = serde_json::json!({
            "payloadType": "application/vnd.assay.pack+yaml;v=1",
            "payload": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, pack_yaml),
            "signatures": [{
                "keyid": "sha256:commercial-key",
                "sig": "dGVzdCBzaWduYXR1cmU="
            }]
        });

        Mock::given(method("GET"))
            .and(path("/packs/commercial-pack/1.0.0.sig"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&envelope))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let result = client
            .fetch_pack_with_signature("commercial-pack", "1.0.0", None)
            .await
            .expect("fetch failed");

        let (fetch, sig) = result.expect("expected Some");

        assert_eq!(fetch.content, pack_yaml);
        assert!(fetch.headers.signature.is_none());
        assert!(sig.is_some());
        assert_eq!(sig.unwrap().signatures[0].key_id, "sha256:commercial-key");
    }

    #[tokio::test]
    async fn test_pack_304_signature_still_valid() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/packs/cached-pack/1.0.0"))
            .and(header("if-none-match", "\"etag-abc\""))
            .respond_with(ResponseTemplate::new(304))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;

        let result = client
            .fetch_pack("cached-pack", "1.0.0", Some("\"etag-abc\""))
            .await
            .expect("fetch failed");

        assert!(
            result.is_none(),
            "304 should return None - use cached pack+signature"
        );
    }

    #[tokio::test]
    async fn test_etag_is_strong_etag_format() {
        let mock_server = MockServer::start().await;

        let pack_yaml = "name: test\nversion: \"1.0.0\"";
        let digest = compute_digest(pack_yaml);
        let etag = format!("\"{}\"", digest);

        Mock::given(method("GET"))
            .and(path("/packs/test/1.0.0"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(pack_yaml)
                    .insert_header("etag", etag.as_str())
                    .insert_header("x-pack-digest", digest.as_str()),
            )
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let result = client.fetch_pack("test", "1.0.0", None).await.unwrap();
        let fetch = result.unwrap();

        assert_eq!(fetch.headers.etag, Some(etag));
        let etag_unquoted = fetch.headers.etag.unwrap().trim_matches('"').to_string();
        assert_eq!(etag_unquoted, digest);
    }

    #[tokio::test]
    async fn test_vary_header_for_authenticated_response() {
        let mock_server = MockServer::start().await;

        let pack_yaml = "name: test\nversion: \"1.0.0\"";

        Mock::given(method("GET"))
            .and(path("/packs/test/1.0.0"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(pack_yaml)
                    .insert_header("vary", "Authorization, Accept-Encoding")
                    .insert_header("cache-control", "private, max-age=86400"),
            )
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let result = client.fetch_pack("test", "1.0.0", None).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_content_digest_vs_canonical_digest() {
        let mock_server = MockServer::start().await;

        let wire_content = "name:   test\nversion:    \"1.0.0\"\n\n";
        let canonical_content = "name: test\nversion: \"1.0.0\"";
        let canonical_digest = compute_digest(canonical_content);

        Mock::given(method("GET"))
            .and(path("/packs/test/1.0.0"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(wire_content)
                    .insert_header("x-pack-digest", canonical_digest.as_str()),
            )
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let result = client.fetch_pack("test", "1.0.0", None).await.unwrap();
        let fetch = result.unwrap();

        assert_eq!(fetch.content, wire_content);
        assert_eq!(fetch.headers.digest, Some(canonical_digest.clone()));
        assert_eq!(fetch.computed_digest, canonical_digest);
    }

    #[tokio::test]
    async fn test_304_cache_hit_flow() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/packs/cached-pack/1.0.0"))
            .and(header("if-none-match", "\"sha256:abc123\""))
            .respond_with(ResponseTemplate::new(304))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;

        let result = client
            .fetch_pack("cached-pack", "1.0.0", Some("\"sha256:abc123\""))
            .await
            .unwrap();

        assert!(result.is_none(), "304 should return None - use cached pack");
    }

    #[tokio::test]
    async fn test_retry_on_429_with_retry_after() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/packs/retry-test/1.0.0"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "1"))
            .expect(2)
            .mount(&mock_server)
            .await;

        let config = RegistryConfig {
            url: mock_server.uri(),
            token: Some("test-token".to_string()),
            max_retries: 1,
            timeout_secs: 30,
            ..Default::default()
        };
        let client = RegistryClient::new(config).unwrap();

        let start = std::time::Instant::now();
        let result = client.fetch_pack("retry-test", "1.0.0", None).await;
        let elapsed = start.elapsed();

        assert!(
            matches!(result, Err(RegistryError::RateLimited { .. })),
            "Should fail with RateLimited"
        );

        assert!(
            elapsed.as_millis() >= 850,
            "Should have waited for retry-after (with jitter), elapsed: {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_max_retries_exceeded() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/packs/fail-test/1.0.0"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "1"))
            .expect(2)
            .mount(&mock_server)
            .await;

        let config = RegistryConfig {
            url: mock_server.uri(),
            token: Some("test-token".to_string()),
            max_retries: 1,
            timeout_secs: 30,
            ..Default::default()
        };
        let client = RegistryClient::new(config).unwrap();

        let result = client.fetch_pack("fail-test", "1.0.0", None).await;
        assert!(
            matches!(result, Err(RegistryError::RateLimited { .. })),
            "Should fail with RateLimited after max retries"
        );
    }
}
