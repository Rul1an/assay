//! API response types for the registry protocol.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Pack metadata returned by the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackMeta {
    /// Pack name (e.g., "eu-ai-act-baseline").
    pub name: String,

    /// Semantic version (e.g., "1.2.0").
    pub version: String,

    /// Pack description.
    #[serde(default)]
    pub description: Option<String>,

    /// Content digest (sha256:...).
    pub digest: String,

    /// Size in bytes.
    #[serde(default)]
    pub size: Option<u64>,

    /// When the pack was published.
    #[serde(default)]
    pub published_at: Option<DateTime<Utc>>,

    /// Whether the pack is signed.
    #[serde(default)]
    pub signed: bool,

    /// Key ID used to sign (if signed).
    #[serde(default)]
    pub key_id: Option<String>,

    /// Whether the pack is deprecated.
    #[serde(default)]
    pub deprecated: bool,

    /// Deprecation message (if deprecated).
    #[serde(default)]
    pub deprecation_message: Option<String>,
}

/// Response from GET /packs/{name}/versions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionsResponse {
    /// Pack name.
    pub name: String,

    /// Available versions, sorted newest first.
    pub versions: Vec<VersionInfo>,
}

/// Version information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    /// Semantic version.
    pub version: String,

    /// Content digest.
    pub digest: String,

    /// When published.
    #[serde(default)]
    pub published_at: Option<DateTime<Utc>>,

    /// Whether deprecated.
    #[serde(default)]
    pub deprecated: bool,
}

/// Headers returned with pack content.
#[derive(Debug, Clone)]
pub struct PackHeaders {
    /// X-Pack-Digest header value.
    pub digest: Option<String>,

    /// X-Pack-Signature header value (Base64 DSSE envelope).
    pub signature: Option<String>,

    /// X-Pack-Key-Id header value.
    pub key_id: Option<String>,

    /// ETag for caching.
    pub etag: Option<String>,

    /// Cache-Control header.
    pub cache_control: Option<String>,

    /// Content-Length.
    pub content_length: Option<u64>,
}

impl PackHeaders {
    /// Parse headers from a response.
    pub fn from_headers(headers: &reqwest::header::HeaderMap) -> Self {
        Self {
            digest: headers
                .get("x-pack-digest")
                .and_then(|v| v.to_str().ok())
                .map(String::from),
            signature: headers
                .get("x-pack-signature")
                .and_then(|v| v.to_str().ok())
                .map(String::from),
            key_id: headers
                .get("x-pack-key-id")
                .and_then(|v| v.to_str().ok())
                .map(String::from),
            etag: headers
                .get(reqwest::header::ETAG)
                .and_then(|v| v.to_str().ok())
                .map(String::from),
            cache_control: headers
                .get(reqwest::header::CACHE_CONTROL)
                .and_then(|v| v.to_str().ok())
                .map(String::from),
            content_length: headers
                .get(reqwest::header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok()),
        }
    }
}

/// Response from GET /keys manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeysManifest {
    /// Schema version.
    pub version: u8,

    /// List of trusted keys.
    pub keys: Vec<TrustedKey>,

    /// When the manifest expires.
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
}

/// A trusted signing key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedKey {
    /// Key ID (sha256:...).
    pub key_id: String,

    /// Algorithm (always "Ed25519" for now).
    pub algorithm: String,

    /// Public key (SPKI DER, Base64).
    pub public_key: String,

    /// Human-readable description.
    #[serde(default)]
    pub description: Option<String>,

    /// When the key was added.
    #[serde(default)]
    pub added_at: Option<DateTime<Utc>>,

    /// When the key expires (if any).
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,

    /// Whether the key is revoked.
    #[serde(default)]
    pub revoked: bool,
}

/// Result of fetching a pack.
#[derive(Debug, Clone)]
pub struct FetchResult {
    /// Pack YAML content.
    pub content: String,

    /// Headers from the response.
    pub headers: PackHeaders,

    /// Computed digest of the content.
    pub computed_digest: String,
}

/// DSSE envelope structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DsseEnvelope {
    /// Payload type (e.g., "application/vnd.assay.pack+yaml;v=1").
    #[serde(rename = "payloadType")]
    pub payload_type: String,

    /// Base64-encoded payload.
    pub payload: String,

    /// Signatures.
    pub signatures: Vec<DsseSignature>,
}

/// DSSE signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DsseSignature {
    /// Key ID.
    #[serde(rename = "keyid")]
    pub key_id: String,

    /// Base64-encoded signature.
    #[serde(rename = "sig")]
    pub signature: String,
}

/// Registry configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    /// Base URL for the registry.
    #[serde(default = "default_registry_url")]
    pub url: String,

    /// Authentication token.
    #[serde(default)]
    pub token: Option<String>,

    /// Whether to allow unsigned packs.
    #[serde(default)]
    pub allow_unsigned: bool,

    /// Request timeout in seconds.
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    /// Maximum retries for transient failures.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

fn default_registry_url() -> String {
    "https://registry.getassay.dev/v1".to_string()
}

fn default_timeout() -> u64 {
    30
}

fn default_max_retries() -> u32 {
    3
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            url: default_registry_url(),
            token: None,
            allow_unsigned: false,
            timeout_secs: default_timeout(),
            max_retries: default_max_retries(),
        }
    }
}

impl RegistryConfig {
    /// Create config from environment variables.
    ///
    /// | Variable | Description |
    /// |----------|-------------|
    /// | `ASSAY_REGISTRY_URL` | Registry base URL |
    /// | `ASSAY_REGISTRY_TOKEN` | Authentication token |
    /// | `ASSAY_ALLOW_UNSIGNED_PACKS` | Allow unsigned packs (dev only) |
    pub fn from_env() -> Self {
        Self {
            url: std::env::var("ASSAY_REGISTRY_URL").unwrap_or_else(|_| default_registry_url()),
            token: std::env::var("ASSAY_REGISTRY_TOKEN").ok(),
            allow_unsigned: std::env::var("ASSAY_ALLOW_UNSIGNED_PACKS")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            timeout_secs: std::env::var("ASSAY_REGISTRY_TIMEOUT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(default_timeout),
            max_retries: std::env::var("ASSAY_REGISTRY_MAX_RETRIES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(default_max_retries),
        }
    }

    /// Set the token.
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// Set the base URL.
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = url.into();
        self
    }

    /// Allow unsigned packs.
    pub fn with_allow_unsigned(mut self, allow: bool) -> Self {
        self.allow_unsigned = allow;
        self
    }
}
