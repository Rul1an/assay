//! Store configuration loader for `.assay/store.yaml`.
//!
//! Provides structured YAML configuration as an alternative to environment
//! variables for evidence store connection settings.
//!
//! # Precedence (frozen in ADR-015 Phase 1 Step 1)
//!
//! **Store URL resolution:**
//! 1. `--store` CLI arg
//! 2. `ASSAY_STORE_URL` env var
//! 3. `url` from config file
//!
//! **Connection overrides:**
//! - `ASSAY_STORE_*` env vars always win over config file values

use serde::Deserialize;
use std::path::{Path, PathBuf};

use super::error::StoreError;
use super::StoreResult;

/// Evidence store configuration from `.assay/store.yaml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoreConfig {
    /// Store URL (e.g., `s3://bucket/prefix`, `file:///path`)
    pub url: String,

    /// Override region (lower priority than `ASSAY_STORE_REGION` env var)
    #[serde(default)]
    pub region: Option<String>,

    /// Allow HTTP connections (lower priority than `ASSAY_STORE_ALLOW_HTTP`)
    #[serde(default)]
    pub allow_http: Option<bool>,

    /// Use path-style URLs (lower priority than `ASSAY_STORE_PATH_STYLE`)
    #[serde(default)]
    pub path_style: Option<bool>,
}

/// Default config file lookup locations, in order.
const DEFAULT_PATHS: &[&str] = &[".assay/store.yaml", "assay-store.yaml"];

impl StoreConfig {
    /// Load config from an explicit path.
    pub fn load(path: &Path) -> StoreResult<Self> {
        let raw = std::fs::read_to_string(path).map_err(|e| StoreError::NotConfigured {
            message: format!("failed to read store config {}: {}", path.display(), e),
        })?;

        serde_yaml::from_str(&raw).map_err(|e| StoreError::NotConfigured {
            message: format!("failed to parse store config {}: {}", path.display(), e),
        })
    }

    /// Discover config from default locations.
    ///
    /// Checks `.assay/store.yaml` and `assay-store.yaml` in order.
    /// Returns `Ok(None)` if no config file is found.
    /// Returns `Err` if a config file exists but fails to parse.
    pub fn discover() -> StoreResult<Option<Self>> {
        for path in DEFAULT_PATHS {
            let p = PathBuf::from(path);
            if p.exists() {
                return Self::load(&p).map(Some);
            }
        }
        Ok(None)
    }

    /// Apply config-file connection overrides to environment, respecting
    /// env-var precedence (env vars always win).
    pub fn apply_env_defaults(&self) {
        if let Some(region) = &self.region {
            if std::env::var("ASSAY_STORE_REGION").is_err() {
                std::env::set_var("ASSAY_STORE_REGION", region);
            }
        }
        if let Some(allow_http) = self.allow_http {
            if std::env::var("ASSAY_STORE_ALLOW_HTTP").is_err() {
                std::env::set_var("ASSAY_STORE_ALLOW_HTTP", if allow_http { "1" } else { "0" });
            }
        }
        if let Some(path_style) = self.path_style {
            if std::env::var("ASSAY_STORE_PATH_STYLE").is_err() {
                std::env::set_var("ASSAY_STORE_PATH_STYLE", if path_style { "1" } else { "0" });
            }
        }
    }
}

/// Resolve the store URL from CLI arg, env var, or config file.
///
/// Follows the frozen precedence from ADR-015 Phase 1 Step 1:
/// 1. `cli_store` (from `--store` or `ASSAY_STORE_URL` via clap)
/// 2. `store_config` file (explicit path or default discovery)
///
/// If a config file is found, its connection overrides are applied.
pub fn resolve_store_url(
    cli_store: Option<&str>,
    store_config_path: Option<&Path>,
) -> StoreResult<String> {
    if let Some(url) = cli_store {
        // Config may still provide connection overrides
        let config = match store_config_path {
            Some(p) => Some(StoreConfig::load(p)?),
            None => StoreConfig::discover()?,
        };
        if let Some(cfg) = config {
            cfg.apply_env_defaults();
        }
        return Ok(url.to_string());
    }

    // Try config file
    let config = match store_config_path {
        Some(p) => Some(StoreConfig::load(p)?),
        None => StoreConfig::discover()?,
    };

    if let Some(cfg) = config {
        cfg.apply_env_defaults();
        return Ok(cfg.url.clone());
    }

    Err(StoreError::NotConfigured {
        message: "no store URL configured (use --store, ASSAY_STORE_URL, or .assay/store.yaml)"
            .to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_load_valid_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store.yaml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "url: s3://my-bucket/evidence").unwrap();
        writeln!(f, "region: us-west-2").unwrap();

        let cfg = StoreConfig::load(&path).unwrap();
        assert_eq!(cfg.url, "s3://my-bucket/evidence");
        assert_eq!(cfg.region, Some("us-west-2".to_string()));
        assert_eq!(cfg.allow_http, None);
        assert_eq!(cfg.path_style, None);
    }

    #[test]
    fn test_load_minimal_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store.yaml");
        std::fs::write(&path, "url: file:///tmp/test-store\n").unwrap();

        let cfg = StoreConfig::load(&path).unwrap();
        assert_eq!(cfg.url, "file:///tmp/test-store");
        assert!(cfg.region.is_none());
    }

    #[test]
    fn test_load_full_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store.yaml");
        std::fs::write(
            &path,
            "url: s3://bucket/prefix\nregion: eu-west-1\nallow_http: true\npath_style: true\n",
        )
        .unwrap();

        let cfg = StoreConfig::load(&path).unwrap();
        assert_eq!(cfg.allow_http, Some(true));
        assert_eq!(cfg.path_style, Some(true));
    }

    #[test]
    fn test_load_rejects_unknown_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store.yaml");
        std::fs::write(&path, "url: s3://b/p\nsecret_key: hunter2\n").unwrap();

        let result = StoreConfig::load(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_cli_wins() {
        let result = resolve_store_url(Some("s3://cli-bucket/prefix"), None).unwrap();
        assert_eq!(result, "s3://cli-bucket/prefix");
    }

    #[test]
    fn test_resolve_config_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store.yaml");
        std::fs::write(&path, "url: s3://config-bucket/prefix\n").unwrap();

        let result = resolve_store_url(None, Some(&path)).unwrap();
        assert_eq!(result, "s3://config-bucket/prefix");
    }

    #[test]
    fn test_resolve_nothing_configured() {
        let result = resolve_store_url(None, None);
        assert!(result.is_err());
    }
}
