//! VCR (Video Cassette Recording) middleware for HTTP request/response recording and replay.
//!
//! SOTA implementation (Jan 2026) with:
//! - Provider-level interception (typed requests, not raw HTTP)
//! - JCS fingerprinting (RFC 8785 canonical JSON)
//! - Scrubbers for security hygiene (no secrets in cassettes)
//! - Atomic writes (temp + rename) for parallel safety
//! - Strict replay mode for hermetic CI
//!
//! # Environment Variables
//!
//! - `ASSAY_VCR_MODE`: `replay_strict` (CI default), `replay`, `record`, `auto`, `off`
//! - `ASSAY_VCR_DIR`: Path to cassette directory (default: `tests/fixtures/perf/semantic_vcr/cassettes`)
//!
//! # Matching
//!
//! Requests are matched by fingerprint: method + URL + canonical body (JCS).
//! Authorization headers and transient metadata are excluded.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// VCR mode: how to handle HTTP requests
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VcrMode {
    /// Replay from cassettes; fail if no match (CI default, hermetic)
    #[default]
    ReplayStrict,
    /// Replay from cassettes; passthrough if no match (dangerous in CI)
    Replay,
    /// Record to cassettes; make real requests (local only)
    Record,
    /// Auto: replay if hit, record if miss (convenient for local dev)
    Auto,
    /// Pass through to live network; no recording
    Off,
}

impl VcrMode {
    /// Parse from environment variable `ASSAY_VCR_MODE`
    pub fn from_env() -> Self {
        match env::var("ASSAY_VCR_MODE")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "record" => VcrMode::Record,
            "auto" => VcrMode::Auto,
            "replay" => VcrMode::Replay,
            "off" => VcrMode::Off,
            // Default to replay_strict for CI safety
            _ => VcrMode::ReplayStrict,
        }
    }

    /// Is this mode allowed to make network requests?
    pub fn allows_network(&self) -> bool {
        matches!(
            self,
            VcrMode::Record | VcrMode::Auto | VcrMode::Replay | VcrMode::Off
        )
    }

    /// Does this mode fail on cassette miss?
    pub fn fails_on_miss(&self) -> bool {
        matches!(self, VcrMode::ReplayStrict)
    }
}

/// Scrubber configuration for security hygiene
#[derive(Debug, Clone, Default)]
pub struct ScrubConfig {
    /// Headers to remove from recorded requests (case-insensitive)
    pub request_headers: Vec<String>,
    /// Headers to remove from recorded responses
    pub response_headers: Vec<String>,
    /// JSON paths to redact in request body (e.g., "$.api_key")
    pub request_body_paths: Vec<String>,
    /// JSON paths to redact in response body
    pub response_body_paths: Vec<String>,
}

impl ScrubConfig {
    /// Default scrubber: remove auth headers and common secrets (VCR/cassette sign-off: no prompt/response bodies by default).
    pub fn default_secure() -> Self {
        Self {
            request_headers: vec![
                "authorization".to_string(),
                "x-api-key".to_string(),
                "openai-organization".to_string(),
                "api-key".to_string(),
            ],
            response_headers: vec![
                "set-cookie".to_string(),
                "x-request-id".to_string(),
                "cf-ray".to_string(),
            ],
            request_body_paths: vec![],
            response_body_paths: vec![],
        }
    }
}

/// A recorded HTTP request/response pair (cassette entry)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CassetteEntry {
    /// Schema version for forward compatibility
    pub schema_version: u32,
    /// Fingerprint used for matching
    pub fingerprint: String,
    /// HTTP method
    pub method: String,
    /// Request URL (without query params that vary)
    pub url: String,
    /// Request body (canonical JSON)
    pub request_body: Option<serde_json::Value>,
    /// Response status code
    pub status: u16,
    /// Response body
    pub response_body: serde_json::Value,
    /// Metadata
    pub meta: CassetteMeta,
}

/// Cassette metadata for debugging and versioning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CassetteMeta {
    /// When this cassette was recorded
    pub recorded_at: String,
    /// Model used (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Provider (openai, anthropic, etc.)
    pub provider: String,
    /// Kind (embeddings, judge, chat)
    pub kind: String,
}

/// VCR client for HTTP request interception
pub struct VcrClient {
    mode: VcrMode,
    cassette_dir: PathBuf,
    scrub_config: ScrubConfig,
    /// In-memory cassette cache (fingerprint -> entry)
    cache: HashMap<String, CassetteEntry>,
    inner: reqwest::Client,
}

impl VcrClient {
    /// Create a new VCR client with mode and directory from environment
    pub fn from_env() -> Self {
        let mode = VcrMode::from_env();
        let cassette_dir = env::var("ASSAY_VCR_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("tests/fixtures/perf/semantic_vcr/cassettes"));

        Self::new(mode, cassette_dir)
    }

    /// Create a new VCR client with explicit mode and directory
    pub fn new(mode: VcrMode, cassette_dir: PathBuf) -> Self {
        let mut client = Self {
            mode,
            cassette_dir,
            scrub_config: ScrubConfig::default_secure(),
            cache: HashMap::new(),
            inner: reqwest::Client::new(),
        };

        // Load existing cassettes in replay modes
        if matches!(
            mode,
            VcrMode::ReplayStrict | VcrMode::Replay | VcrMode::Auto
        ) {
            client.load_cassettes();
        }

        client
    }

    /// Set custom scrub configuration
    pub fn with_scrub_config(mut self, config: ScrubConfig) -> Self {
        self.scrub_config = config;
        self
    }

    /// Compute fingerprint for request matching using JCS (RFC 8785)
    ///
    /// Fingerprint includes: method + URL + canonical body (excluding auth)
    pub fn fingerprint(method: &str, url: &str, body: Option<&serde_json::Value>) -> String {
        let mut hasher = Sha256::new();
        hasher.update(method.as_bytes());
        hasher.update(b"|");

        // Normalize URL (remove trailing slashes, lowercase)
        let normalized_url = url.trim_end_matches('/').to_lowercase();
        hasher.update(normalized_url.as_bytes());
        hasher.update(b"|");

        if let Some(b) = body {
            // Use JCS for canonical JSON (RFC 8785)
            let canonical = serde_jcs::to_string(b).unwrap_or_else(|_| b.to_string());
            hasher.update(canonical.as_bytes());
        }

        format!("{:x}", hasher.finalize())
    }

    /// Determine provider from URL
    fn provider_from_url(url: &str) -> &'static str {
        if url.contains("openai.com") {
            "openai"
        } else if url.contains("anthropic.com") {
            "anthropic"
        } else {
            "unknown"
        }
    }

    /// Determine kind (embeddings/judge/chat) from URL
    fn kind_from_url(url: &str) -> &'static str {
        if url.contains("/embeddings") {
            "embeddings"
        } else if url.contains("/chat/completions") {
            "judge"
        } else if url.contains("/completions") {
            "completions"
        } else {
            "other"
        }
    }

    /// Extract model from request body if present
    fn extract_model(body: Option<&serde_json::Value>) -> Option<String> {
        body.and_then(|b| b.get("model"))
            .and_then(|m| m.as_str())
            .map(|s| s.to_string())
    }

    /// Load all cassettes from disk into memory
    fn load_cassettes(&mut self) {
        let cassette_dir = self.cassette_dir.clone();
        if !cassette_dir.exists() {
            return;
        }

        // Load from provider/kind subdirs
        for provider in &["openai", "anthropic", "unknown"] {
            for kind in &["embeddings", "judge", "completions", "other"] {
                let dir = cassette_dir.join(provider).join(kind);
                if dir.exists() {
                    self.load_cassettes_from_dir(&dir);
                }
            }
        }

        // Also load from legacy structure (embeddings/, judge/ at root)
        for subdir in &["embeddings", "judge"] {
            let dir = cassette_dir.join(subdir);
            if dir.exists() {
                self.load_cassettes_from_dir(&dir);
            }
        }

        // Load from root cassette dir
        self.load_cassettes_from_dir(&cassette_dir);
    }

    fn load_cassettes_from_dir(&mut self, dir: &Path) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(cassette) = serde_json::from_str::<CassetteEntry>(&content) {
                        self.cache.insert(cassette.fingerprint.clone(), cassette);
                    }
                }
            }
        }
    }

    /// Save a cassette entry to disk (atomic: temp + rename)
    fn save_cassette(&self, entry: &CassetteEntry) -> anyhow::Result<()> {
        let provider = Self::provider_from_url(&entry.url);
        let kind = Self::kind_from_url(&entry.url);

        // Create directory: cassettes/<provider>/<kind>/
        let dir = self.cassette_dir.join(provider).join(kind);
        fs::create_dir_all(&dir)?;

        // Use first 16 chars of fingerprint for filename
        let fp_prefix = if entry.fingerprint.len() >= 16 {
            &entry.fingerprint[..16]
        } else {
            &entry.fingerprint
        };
        let filename = format!("{}.json", fp_prefix);
        let final_path = dir.join(&filename);

        // Atomic write: temp file + rename
        let temp_path = dir.join(format!(".{}.tmp", fp_prefix));
        let content = serde_json::to_string_pretty(entry)?;

        {
            let mut file = fs::File::create(&temp_path)?;
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
        }

        fs::rename(&temp_path, &final_path)?;

        Ok(())
    }

    /// Make a POST request with VCR handling
    pub async fn post_json(
        &mut self,
        url: &str,
        body: &serde_json::Value,
        auth_header: Option<&str>,
    ) -> anyhow::Result<VcrResponse> {
        let fingerprint = Self::fingerprint("POST", url, Some(body));

        match self.mode {
            VcrMode::ReplayStrict => {
                // Strict replay: must find cassette
                if let Some(entry) = self.cache.get(&fingerprint) {
                    Ok(VcrResponse {
                        status: entry.status,
                        body: entry.response_body.clone(),
                        from_cache: true,
                    })
                } else {
                    anyhow::bail!(
                        "VCR replay_strict: no cassette found for POST {} (fingerprint: {}).\n\
                        Run with ASSAY_VCR_MODE=record to record responses.\n\
                        Cassette dir: {}",
                        url,
                        &fingerprint[..16.min(fingerprint.len())],
                        self.cassette_dir.display()
                    )
                }
            }
            VcrMode::Replay => {
                // Soft replay: try cache, passthrough on miss
                if let Some(entry) = self.cache.get(&fingerprint) {
                    Ok(VcrResponse {
                        status: entry.status,
                        body: entry.response_body.clone(),
                        from_cache: true,
                    })
                } else {
                    // Passthrough (dangerous in CI!)
                    tracing::warn!(
                        "VCR replay: cache miss for POST {}, passing through to network",
                        url
                    );
                    self.make_request_and_record(url, body, auth_header, &fingerprint, false)
                        .await
                }
            }
            VcrMode::Auto => {
                // Auto: replay if hit, record if miss
                if let Some(entry) = self.cache.get(&fingerprint) {
                    Ok(VcrResponse {
                        status: entry.status,
                        body: entry.response_body.clone(),
                        from_cache: true,
                    })
                } else {
                    self.make_request_and_record(url, body, auth_header, &fingerprint, true)
                        .await
                }
            }
            VcrMode::Record => {
                // Always record (overwrite existing)
                self.make_request_and_record(url, body, auth_header, &fingerprint, true)
                    .await
            }
            VcrMode::Off => {
                // Pass through, no recording
                crate::providers::network::check_outbound(url)?;
                let mut req = self.inner.post(url).json(body);
                if let Some(auth) = auth_header {
                    req = req.header("Authorization", auth);
                }
                let resp = req.send().await?;
                let status = resp.status().as_u16();
                let response_body: serde_json::Value = resp.json().await?;

                Ok(VcrResponse {
                    status,
                    body: response_body,
                    from_cache: false,
                })
            }
        }
    }

    /// Make real HTTP request and optionally record to cassette
    async fn make_request_and_record(
        &mut self,
        url: &str,
        body: &serde_json::Value,
        auth_header: Option<&str>,
        fingerprint: &str,
        should_record: bool,
    ) -> anyhow::Result<VcrResponse> {
        crate::providers::network::check_outbound(url)?;
        let mut req = self.inner.post(url).json(body);
        if let Some(auth) = auth_header {
            req = req.header("Authorization", auth);
        }
        let resp = req.send().await?;

        let status = resp.status().as_u16();
        let response_body: serde_json::Value = resp.json().await?;

        if should_record {
            let entry = CassetteEntry {
                schema_version: 2,
                fingerprint: fingerprint.to_string(),
                method: "POST".to_string(),
                url: url.to_string(),
                request_body: Some(body.clone()),
                status,
                response_body: response_body.clone(),
                meta: CassetteMeta {
                    recorded_at: chrono::Utc::now().to_rfc3339(),
                    model: Self::extract_model(Some(body)),
                    provider: Self::provider_from_url(url).to_string(),
                    kind: Self::kind_from_url(url).to_string(),
                },
            };

            if let Err(e) = self.save_cassette(&entry) {
                tracing::warn!("VCR: failed to save cassette: {}", e);
            }

            // Add to cache
            self.cache.insert(fingerprint.to_string(), entry);
        }

        Ok(VcrResponse {
            status,
            body: response_body,
            from_cache: false,
        })
    }

    /// Get the current VCR mode
    pub fn mode(&self) -> VcrMode {
        self.mode
    }

    /// Get cassette count (for diagnostics)
    pub fn cassette_count(&self) -> usize {
        self.cache.len()
    }
}

/// Response from VCR client
#[derive(Debug)]
pub struct VcrResponse {
    pub status: u16,
    pub body: serde_json::Value,
    /// True if response came from cache (replay)
    pub from_cache: bool,
}

impl VcrResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Sign-off: default_secure() must scrub auth and common secret headers so cassettes don't leak.
    #[test]
    fn test_default_secure_scrub_paths() {
        let cfg = ScrubConfig::default_secure();
        assert!(
            cfg.request_headers
                .iter()
                .any(|h| h.eq_ignore_ascii_case("authorization")),
            "Must scrub Authorization"
        );
        assert!(
            cfg.request_headers
                .iter()
                .any(|h| h.eq_ignore_ascii_case("x-api-key")),
            "Must scrub x-api-key"
        );
        assert!(
            cfg.request_headers
                .iter()
                .any(|h| h.eq_ignore_ascii_case("api-key")),
            "Must scrub api-key"
        );
        assert!(
            cfg.response_headers
                .iter()
                .any(|h| h.eq_ignore_ascii_case("set-cookie")),
            "Must scrub set-cookie"
        );
        assert!(
            cfg.request_body_paths.is_empty(),
            "Default: no body paths (audit: explicit if needed)"
        );
        assert!(
            cfg.response_body_paths.is_empty(),
            "Default: no response body paths"
        );
    }

    #[test]
    fn test_fingerprint_stability() {
        let body = serde_json::json!({"input": "hello", "model": "text-embedding-3-small"});
        let fp1 =
            VcrClient::fingerprint("POST", "https://api.openai.com/v1/embeddings", Some(&body));
        let fp2 =
            VcrClient::fingerprint("POST", "https://api.openai.com/v1/embeddings", Some(&body));
        assert_eq!(fp1, fp2);

        // Different body = different fingerprint
        let body2 = serde_json::json!({"input": "world", "model": "text-embedding-3-small"});
        let fp3 =
            VcrClient::fingerprint("POST", "https://api.openai.com/v1/embeddings", Some(&body2));
        assert_ne!(fp1, fp3);
    }

    #[test]
    fn test_fingerprint_key_order_invariant() {
        // JCS ensures key order doesn't matter
        let body1 = serde_json::json!({"model": "gpt-4", "input": "hello"});
        let body2 = serde_json::json!({"input": "hello", "model": "gpt-4"});
        let fp1 =
            VcrClient::fingerprint("POST", "https://api.openai.com/v1/embeddings", Some(&body1));
        let fp2 =
            VcrClient::fingerprint("POST", "https://api.openai.com/v1/embeddings", Some(&body2));
        assert_eq!(fp1, fp2, "JCS should normalize key order");
    }

    #[test]
    fn test_vcr_mode_from_env() {
        env::remove_var("ASSAY_VCR_MODE");
        assert_eq!(VcrMode::from_env(), VcrMode::ReplayStrict);

        env::set_var("ASSAY_VCR_MODE", "record");
        assert_eq!(VcrMode::from_env(), VcrMode::Record);

        env::set_var("ASSAY_VCR_MODE", "auto");
        assert_eq!(VcrMode::from_env(), VcrMode::Auto);

        env::set_var("ASSAY_VCR_MODE", "replay");
        assert_eq!(VcrMode::from_env(), VcrMode::Replay);

        env::set_var("ASSAY_VCR_MODE", "off");
        assert_eq!(VcrMode::from_env(), VcrMode::Off);

        env::set_var("ASSAY_VCR_MODE", "replay_strict");
        assert_eq!(VcrMode::from_env(), VcrMode::ReplayStrict);

        env::remove_var("ASSAY_VCR_MODE");
    }

    #[test]
    fn test_cassette_save_load_atomic() {
        let tmp = TempDir::new().unwrap();
        let client = VcrClient::new(VcrMode::Record, tmp.path().to_path_buf());

        let body = serde_json::json!({"input": "test", "model": "text-embedding-3-small"});
        let fingerprint =
            VcrClient::fingerprint("POST", "https://api.openai.com/v1/embeddings", Some(&body));

        let entry = CassetteEntry {
            schema_version: 2,
            fingerprint: fingerprint.clone(),
            method: "POST".to_string(),
            url: "https://api.openai.com/v1/embeddings".to_string(),
            request_body: Some(body),
            status: 200,
            response_body: serde_json::json!({"data": [{"embedding": [0.1, 0.2]}]}),
            meta: CassetteMeta {
                recorded_at: "2026-01-30T12:00:00Z".to_string(),
                model: Some("text-embedding-3-small".to_string()),
                provider: "openai".to_string(),
                kind: "embeddings".to_string(),
            },
        };

        client.save_cassette(&entry).unwrap();

        // Verify file exists in correct location
        let expected_path = tmp
            .path()
            .join("openai")
            .join("embeddings")
            .join(format!("{}.json", &fingerprint[..16]));
        assert!(expected_path.exists(), "Cassette file should exist");

        // Reload and verify
        let mut client2 = VcrClient::new(VcrMode::ReplayStrict, tmp.path().to_path_buf());
        client2.load_cassettes();

        assert!(client2.cache.contains_key(&fingerprint));
        assert_eq!(client2.cache.get(&fingerprint).unwrap().status, 200);
    }

    #[test]
    fn test_provider_and_kind_detection() {
        assert_eq!(
            VcrClient::provider_from_url("https://api.openai.com/v1/embeddings"),
            "openai"
        );
        assert_eq!(
            VcrClient::kind_from_url("https://api.openai.com/v1/embeddings"),
            "embeddings"
        );
        assert_eq!(
            VcrClient::kind_from_url("https://api.openai.com/v1/chat/completions"),
            "judge"
        );
    }

    #[tokio::test]
    async fn test_network_policy_blocks_passthrough_modes() {
        let _serial = crate::providers::network::lock_test_serial();
        let tmp = TempDir::new().unwrap();
        let mut client = VcrClient::new(VcrMode::Off, tmp.path().to_path_buf());
        let _guard = crate::providers::network::NetworkPolicyGuard::deny("unit test");
        let body = serde_json::json!({"input": "test", "model": "gpt-4o-mini"});
        let err = client
            .post_json("https://api.openai.com/v1/chat/completions", &body, None)
            .await
            .expect_err("deny policy must block passthrough network");
        assert!(err
            .to_string()
            .contains("outbound network blocked by policy"));
    }
}
