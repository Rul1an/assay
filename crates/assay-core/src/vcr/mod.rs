//! VCR (Video Cassette Recording) middleware for HTTP request/response recording and replay.
//!
//! Used for deterministic testing of semantic/judge workloads without network variance.
//!
//! # Environment Variables
//!
//! - `ASSAY_VCR_MODE`: `replay` (default in CI), `record` (local), `off` (live network)
//! - `ASSAY_VCR_DIR`: Path to cassette directory (default: `tests/fixtures/perf/semantic_vcr/cassettes`)
//!
//! # Matching
//!
//! Requests are matched by: method + URL + body (canonicalized JSON). Authorization headers
//! are excluded from matching to avoid secrets in cassettes.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// VCR mode: how to handle HTTP requests
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VcrMode {
    /// Replay from cassettes; fail if no match (CI default)
    #[default]
    Replay,
    /// Record to cassettes; make real requests (local only)
    Record,
    /// Pass through to live network; no recording (testing)
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
            "off" => VcrMode::Off,
            _ => VcrMode::Replay, // default
        }
    }
}

/// A recorded HTTP request/response pair
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CassetteEntry {
    pub method: String,
    pub url: String,
    /// Request body (JSON, canonicalized for matching)
    pub request_body: Option<serde_json::Value>,
    /// Response status code
    pub status: u16,
    /// Response body
    pub response_body: serde_json::Value,
    /// Fingerprint used for matching (method + url + canonicalized body hash)
    pub fingerprint: String,
}

/// VCR client for HTTP request interception
pub struct VcrClient {
    mode: VcrMode,
    cassette_dir: PathBuf,
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
            cache: HashMap::new(),
            inner: reqwest::Client::new(),
        };

        // Load existing cassettes in replay mode
        if mode == VcrMode::Replay {
            client.load_cassettes();
        }

        client
    }

    /// Compute fingerprint for request matching (excludes Authorization header)
    pub fn fingerprint(method: &str, url: &str, body: Option<&serde_json::Value>) -> String {
        let mut hasher = Sha256::new();
        hasher.update(method.as_bytes());
        hasher.update(b"|");
        hasher.update(url.as_bytes());
        hasher.update(b"|");

        if let Some(b) = body {
            // Canonicalize JSON for stable hashing
            let canonical = serde_jcs::to_string(b).unwrap_or_else(|_| b.to_string());
            hasher.update(canonical.as_bytes());
        }

        format!("{:x}", hasher.finalize())
    }

    /// Load all cassettes from disk into memory
    fn load_cassettes(&mut self) {
        let cassette_dir = self.cassette_dir.clone();
        if !cassette_dir.exists() {
            return;
        }

        // Load from embeddings/ and judge/ subdirs
        for subdir in &["embeddings", "judge"] {
            let dir = cassette_dir.join(subdir);
            if dir.exists() {
                self.load_cassettes_from_dir(&dir);
            }
        }

        // Also load from root cassette dir
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

    /// Save a cassette entry to disk
    fn save_cassette(&self, entry: &CassetteEntry, category: &str) -> anyhow::Result<()> {
        let dir = self.cassette_dir.join(category);
        fs::create_dir_all(&dir)?;

        // Use first 16 chars of fingerprint, or full fingerprint if shorter
        let fp_prefix = if entry.fingerprint.len() >= 16 {
            &entry.fingerprint[..16]
        } else {
            &entry.fingerprint
        };
        let filename = format!("{}.json", fp_prefix);
        let path = dir.join(filename);

        let content = serde_json::to_string_pretty(entry)?;
        fs::write(path, content)?;

        Ok(())
    }

    /// Determine category (embeddings/judge) from URL
    fn category_from_url(url: &str) -> &'static str {
        if url.contains("/embeddings") {
            "embeddings"
        } else {
            "judge"
        }
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
            VcrMode::Replay => {
                // Look up in cache
                if let Some(entry) = self.cache.get(&fingerprint) {
                    Ok(VcrResponse {
                        status: entry.status,
                        body: entry.response_body.clone(),
                    })
                } else {
                    anyhow::bail!(
                        "VCR replay: no cassette found for POST {} (fingerprint: {}). \
                        Run with ASSAY_VCR_MODE=record to record responses.",
                        url,
                        &fingerprint[..16]
                    )
                }
            }
            VcrMode::Record => {
                // Make real request
                let mut req = self.inner.post(url).json(body);
                if let Some(auth) = auth_header {
                    req = req.header("Authorization", auth);
                }
                let resp = req.send().await?;

                let status = resp.status().as_u16();
                let response_body: serde_json::Value = resp.json().await?;

                // Save to cassette
                let entry = CassetteEntry {
                    method: "POST".to_string(),
                    url: url.to_string(),
                    request_body: Some(body.clone()),
                    status,
                    response_body: response_body.clone(),
                    fingerprint: fingerprint.clone(),
                };

                let category = Self::category_from_url(url);
                if let Err(e) = self.save_cassette(&entry, category) {
                    tracing::warn!("VCR: failed to save cassette: {}", e);
                }

                // Add to cache
                self.cache.insert(fingerprint, entry);

                Ok(VcrResponse {
                    status,
                    body: response_body,
                })
            }
            VcrMode::Off => {
                // Pass through to live network
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
                })
            }
        }
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
    fn test_vcr_mode_from_env() {
        // Default = Replay
        env::remove_var("ASSAY_VCR_MODE");
        assert_eq!(VcrMode::from_env(), VcrMode::Replay);

        env::set_var("ASSAY_VCR_MODE", "record");
        assert_eq!(VcrMode::from_env(), VcrMode::Record);

        env::set_var("ASSAY_VCR_MODE", "off");
        assert_eq!(VcrMode::from_env(), VcrMode::Off);

        env::set_var("ASSAY_VCR_MODE", "REPLAY");
        assert_eq!(VcrMode::from_env(), VcrMode::Replay);

        env::remove_var("ASSAY_VCR_MODE");
    }

    #[test]
    fn test_cassette_save_load() {
        let tmp = TempDir::new().unwrap();
        let client = VcrClient::new(VcrMode::Record, tmp.path().to_path_buf());

        // Use a real fingerprint (SHA256 = 64 hex chars)
        let body = serde_json::json!({"input": "test"});
        let fingerprint =
            VcrClient::fingerprint("POST", "https://api.openai.com/v1/embeddings", Some(&body));

        let entry = CassetteEntry {
            method: "POST".to_string(),
            url: "https://api.openai.com/v1/embeddings".to_string(),
            request_body: Some(body),
            status: 200,
            response_body: serde_json::json!({"data": [{"embedding": [0.1, 0.2]}]}),
            fingerprint: fingerprint.clone(),
        };

        client.save_cassette(&entry, "embeddings").unwrap();

        // Reload
        let mut client2 = VcrClient::new(VcrMode::Replay, tmp.path().to_path_buf());
        client2.load_cassettes();

        assert!(client2.cache.contains_key(&fingerprint));
    }
}
