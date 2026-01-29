//! HTTP client for the pack registry.

use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, IF_NONE_MATCH, USER_AGENT};
use tracing::{debug, warn};

use crate::auth::TokenProvider;
use crate::canonicalize::compute_canonical_digest;
use crate::error::{RegistryError, RegistryResult};
use crate::types::{
    DsseEnvelope, FetchResult, KeysManifest, PackHeaders, PackMeta, RegistryConfig,
    VersionsResponse,
};

/// User agent for registry requests.
const USER_AGENT_VALUE: &str = concat!("assay-registry/", env!("CARGO_PKG_VERSION"));

/// Registry client for fetching packs.
#[derive(Debug, Clone)]
pub struct RegistryClient {
    /// HTTP client.
    client: reqwest::Client,

    /// Base URL for the registry.
    base_url: String,

    /// Token provider for authentication.
    token_provider: TokenProvider,

    /// Configuration.
    config: RegistryConfig,
}

impl RegistryClient {
    /// Create a new registry client.
    pub fn new(config: RegistryConfig) -> RegistryResult<Self> {
        let token_provider = config
            .token
            .as_ref()
            .map(TokenProvider::static_token)
            .unwrap_or_else(TokenProvider::from_env);

        Self::with_token_provider(config, token_provider)
    }

    /// Create a client with a custom token provider.
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

        // Normalize base URL (remove trailing slash)
        let base_url = config.url.trim_end_matches('/').to_string();

        Ok(Self {
            client,
            base_url,
            token_provider,
            config,
        })
    }

    /// Create a client from environment variables.
    pub fn from_env() -> RegistryResult<Self> {
        Self::new(RegistryConfig::from_env())
    }

    /// Get pack metadata (HEAD request).
    ///
    /// Returns metadata without downloading content.
    pub async fn get_pack_meta(&self, name: &str, version: &str) -> RegistryResult<PackMeta> {
        let url = format!("{}/packs/{}/{}", self.base_url, name, version);
        debug!(url = %url, "fetching pack metadata");

        let response = self.request(reqwest::Method::HEAD, &url, None).await?;

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

    /// Fetch pack content.
    ///
    /// Returns the pack YAML content and headers.
    pub async fn fetch_pack(
        &self,
        name: &str,
        version: &str,
        etag: Option<&str>,
    ) -> RegistryResult<Option<FetchResult>> {
        let url = format!("{}/packs/{}/{}", self.base_url, name, version);
        debug!(url = %url, etag = ?etag, "fetching pack content");

        let response = self.request(reqwest::Method::GET, &url, etag).await?;

        // Check for 304 Not Modified
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            debug!("pack not modified (304)");
            return Ok(None);
        }

        let headers = PackHeaders::from_headers(response.headers());
        let content = response.text().await.map_err(|e| RegistryError::Network {
            message: format!("failed to read response body: {}", e),
        })?;

        // Compute digest of content
        let computed_digest = compute_digest(&content);

        Ok(Some(FetchResult {
            content,
            headers,
            computed_digest,
        }))
    }

    /// List available versions for a pack.
    pub async fn list_versions(&self, name: &str) -> RegistryResult<VersionsResponse> {
        let url = format!("{}/packs/{}/versions", self.base_url, name);
        debug!(url = %url, "listing pack versions");

        let response = self.request(reqwest::Method::GET, &url, None).await?;

        response
            .json()
            .await
            .map_err(|e| RegistryError::InvalidResponse {
                message: format!("failed to parse versions response: {}", e),
            })
    }

    /// Fetch the keys manifest.
    pub async fn fetch_keys(&self) -> RegistryResult<KeysManifest> {
        let url = format!("{}/keys", self.base_url);
        debug!(url = %url, "fetching keys manifest");

        let response = self.request(reqwest::Method::GET, &url, None).await?;

        response
            .json()
            .await
            .map_err(|e| RegistryError::InvalidResponse {
                message: format!("failed to parse keys manifest: {}", e),
            })
    }

    /// Fetch signature from sidecar endpoint (SPEC §7.3).
    ///
    /// The signature is delivered separately from the pack content to avoid
    /// header size limits. The sidecar returns a DSSE envelope.
    ///
    /// Endpoint: GET /packs/{name}/{version}.sig
    pub async fn fetch_signature(
        &self,
        name: &str,
        version: &str,
    ) -> RegistryResult<Option<DsseEnvelope>> {
        let url = format!("{}/packs/{}/{}.sig", self.base_url, name, version);
        debug!(url = %url, "fetching signature from sidecar");

        let response = match self.request(reqwest::Method::GET, &url, None).await {
            Ok(response) => response,
            Err(RegistryError::NotFound { .. }) => {
                // Signature sidecar may not exist for unsigned packs
                debug!("signature sidecar not found (pack may be unsigned)");
                return Ok(None);
            }
            Err(e) => return Err(e),
        };

        let envelope: DsseEnvelope =
            response
                .json()
                .await
                .map_err(|e| RegistryError::InvalidResponse {
                    message: format!("failed to parse signature envelope: {}", e),
                })?;

        Ok(Some(envelope))
    }

    /// Fetch pack with signature from sidecar (recommended for signed packs).
    ///
    /// This method fetches both the pack content and its signature in parallel,
    /// avoiding header size limits from DSSE-in-header.
    pub async fn fetch_pack_with_signature(
        &self,
        name: &str,
        version: &str,
        etag: Option<&str>,
    ) -> RegistryResult<Option<(FetchResult, Option<DsseEnvelope>)>> {
        // Fetch pack content
        let pack_result = self.fetch_pack(name, version, etag).await?;

        let fetch = match pack_result {
            Some(f) => f,
            None => return Ok(None), // 304 Not Modified
        };

        // Fetch signature from sidecar (don't fail if missing)
        let signature = self.fetch_signature(name, version).await.ok().flatten();

        Ok(Some((fetch, signature)))
    }

    /// Make an authenticated request with retry and rate limit handling.
    async fn request(
        &self,
        method: reqwest::Method,
        url: &str,
        etag: Option<&str>,
    ) -> RegistryResult<reqwest::Response> {
        let mut retries = 0;
        let max_retries = self.config.max_retries;

        loop {
            let result = self.request_once(method.clone(), url, etag).await;

            match result {
                Ok(response) => return Ok(response),
                Err(e) if e.is_retryable() && retries < max_retries => {
                    retries += 1;

                    // Calculate backoff with exponential increase
                    let backoff = match &e {
                        RegistryError::RateLimited { retry_after } => {
                            retry_after.unwrap_or(Duration::from_secs(1 << retries))
                        }
                        _ => Duration::from_secs(1 << retries),
                    };

                    // Cap at 30 seconds
                    let backoff = backoff.min(Duration::from_secs(30));

                    warn!(
                        error = %e,
                        retry = retries,
                        max_retries = max_retries,
                        backoff_secs = backoff.as_secs(),
                        "retrying request"
                    );

                    tokio::time::sleep(backoff).await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Make a single request without retry.
    async fn request_once(
        &self,
        method: reqwest::Method,
        url: &str,
        etag: Option<&str>,
    ) -> RegistryResult<reqwest::Response> {
        let mut request = self.client.request(method, url);

        // Add authentication if available
        if let Some(token) = self.token_provider.get_token().await? {
            request = request.header(AUTHORIZATION, format!("Bearer {}", token));
        }

        // Add ETag for conditional requests
        if let Some(etag) = etag {
            request = request.header(IF_NONE_MATCH, etag);
        }

        let response = request.send().await?;
        let status = response.status();

        // Handle error status codes
        match status.as_u16() {
            200..=299 | 304 => Ok(response),

            401 => Err(RegistryError::Unauthorized {
                message: "invalid or expired token".to_string(),
            }),

            404 => {
                // Extract name and version from URL for better error message
                let (name, version) = parse_pack_url(url);
                Err(RegistryError::NotFound { name, version })
            }

            410 => {
                // Pack revoked
                let (name, version) = parse_pack_url(url);
                let reason = response
                    .headers()
                    .get("x-revocation-reason")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("no reason provided")
                    .to_string();

                Err(RegistryError::Revoked {
                    name,
                    version,
                    reason,
                })
            }

            429 => {
                // Rate limited - extract Retry-After
                let retry_after = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .map(Duration::from_secs);

                Err(RegistryError::RateLimited { retry_after })
            }

            _ => {
                let message = response.text().await.unwrap_or_else(|_| status.to_string());
                Err(RegistryError::Network {
                    message: format!("HTTP {}: {}", status.as_u16(), message),
                })
            }
        }
    }

    /// Get the base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Check if authenticated.
    pub fn is_authenticated(&self) -> bool {
        self.token_provider.is_authenticated()
    }
}

/// Compute canonical digest of content per SPEC §6.2.
///
/// Uses JCS canonicalization for valid YAML, falls back to raw SHA-256 for
/// non-YAML content (e.g., error responses).
fn compute_digest(content: &str) -> String {
    match compute_canonical_digest(content) {
        Ok(digest) => digest,
        Err(_) => {
            // Fall back to raw digest for non-YAML content
            use sha2::{Digest, Sha256};
            let hash = Sha256::digest(content.as_bytes());
            format!("sha256:{:x}", hash)
        }
    }
}

/// Parse pack name and version from URL.
fn parse_pack_url(url: &str) -> (String, String) {
    // URL format: .../packs/{name}/{version}
    let parts: Vec<&str> = url.split('/').collect();
    let len = parts.len();

    if len >= 2 {
        (
            parts.get(len - 2).unwrap_or(&"unknown").to_string(),
            parts.get(len - 1).unwrap_or(&"unknown").to_string(),
        )
    } else {
        ("unknown".to_string(), "unknown".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_digest_canonical() {
        // Valid YAML uses canonical JCS digest
        let content = "name: test\nversion: \"1.0.0\"";
        let digest = compute_digest(content);
        assert!(digest.starts_with("sha256:"));
        assert_eq!(digest.len(), 7 + 64);

        // Should match the canonical digest
        let expected = compute_canonical_digest(content).unwrap();
        assert_eq!(digest, expected);
    }

    #[test]
    fn test_compute_digest_non_yaml_fallback() {
        // Invalid YAML falls back to raw digest
        let content = "this is not: valid: yaml: [[";
        let digest = compute_digest(content);
        assert!(digest.starts_with("sha256:"));
        assert_eq!(digest.len(), 7 + 64);
    }

    #[test]
    fn test_parse_pack_url() {
        let url = "https://registry.getassay.dev/v1/packs/eu-ai-act/1.2.0";
        let (name, version) = parse_pack_url(url);
        assert_eq!(name, "eu-ai-act");
        assert_eq!(version, "1.2.0");
    }

    #[test]
    fn test_config_from_env_defaults() {
        // Clear env vars
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

        // Use valid YAML that will parse correctly for canonical digest
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
    async fn test_fetch_pack_revoked() {
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
            }) => {
                assert_eq!(name, "revoked-pack");
                assert_eq!(version, "1.0.0");
                assert_eq!(reason, "security vulnerability");
            }
            _ => panic!("expected Revoked error"),
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

        // Create client with no retries to test immediate error
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

        // This mock requires the auth header
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

        // This mock should NOT have auth header
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

        // Mock pack endpoint
        Mock::given(method("GET"))
            .and(path("/packs/signed-pack/1.0.0"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(pack_yaml)
                    .insert_header("x-pack-digest", expected_digest.as_str()),
            )
            .mount(&mock_server)
            .await;

        // Mock signature sidecar
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
        // Scenario: Commercial pack has NO X-Pack-Signature header
        // Client MUST fetch signature from sidecar endpoint
        let mock_server = MockServer::start().await;

        let pack_yaml = "name: commercial-pack\nversion: \"1.0.0\"\nlicense: commercial";
        let expected_digest = compute_digest(pack_yaml);

        // Pack endpoint returns NO X-Pack-Signature header (header too large or policy)
        Mock::given(method("GET"))
            .and(path("/packs/commercial-pack/1.0.0"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(pack_yaml)
                    .insert_header("x-pack-digest", expected_digest.as_str())
                    .insert_header("x-pack-license", "LicenseRef-Assay-Enterprise-1.0")
                    // NOTE: No X-Pack-Signature header!
                    .insert_header(
                        "x-pack-signature-endpoint",
                        "/packs/commercial-pack/1.0.0.sig",
                    ),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        // Signature MUST be fetched from sidecar
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
            .expect(1) // MUST be called
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;
        let result = client
            .fetch_pack_with_signature("commercial-pack", "1.0.0", None)
            .await
            .expect("fetch failed");

        let (fetch, sig) = result.expect("expected Some");

        // Verify pack fetched
        assert_eq!(fetch.content, pack_yaml);

        // Verify signature was fetched from sidecar (not header)
        assert!(
            fetch.headers.signature.is_none(),
            "header signature should be absent"
        );
        assert!(sig.is_some(), "sidecar signature MUST be present");
        assert_eq!(sig.unwrap().signatures[0].key_id, "sha256:commercial-key");
    }

    #[tokio::test]
    async fn test_pack_304_signature_still_valid() {
        // Scenario: Pack returns 304 Not Modified
        // Question: Does signature need refetch?
        // Policy: No - if pack unchanged, signature unchanged (same digest binding)
        let mock_server = MockServer::start().await;

        // First fetch: get pack + signature
        let pack_yaml = "name: cached-pack\nversion: \"1.0.0\"";
        let _expected_digest = compute_digest(pack_yaml); // Unused in 304 test

        Mock::given(method("GET"))
            .and(path("/packs/cached-pack/1.0.0"))
            .and(header("if-none-match", "\"etag-abc\""))
            .respond_with(ResponseTemplate::new(304)) // Not Modified
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server).await;

        // When pack is 304, fetch_pack returns None
        let result = client
            .fetch_pack("cached-pack", "1.0.0", Some("\"etag-abc\""))
            .await
            .expect("fetch failed");

        // 304 means: use cached pack AND cached signature
        // No need to refetch signature - digest binding unchanged
        assert!(
            result.is_none(),
            "304 should return None - use cached pack+signature"
        );

        // Document the policy: signature cache is tied to pack ETag/digest
        // If pack unchanged (304), signature is also unchanged
    }
}
