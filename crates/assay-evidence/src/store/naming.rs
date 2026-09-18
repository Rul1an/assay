//! Key naming conventions for bundle storage.
//!
//! # Key Schema (Simplified)
//!
//! ```text
//! {base_prefix}/bundles/{bundle_id}.tar.gz      # Primary (content-addressed)
//! {base_prefix}/runs/{run_id}/{bundle_id}.ref   # Run index (small ref file)
//! ```
//!
//! ## Why this structure?
//!
//! 1. **Content-addressed keys**: `bundles/{bundle_id}.tar.gz` is the single
//!    source of truth. Hash prefix distributes well across S3 partitions.
//!
//! 2. **O(1) operations**:
//!    - `pull --bundle-id`: Direct key lookup
//!    - `list --run-id`: Prefix listing on `runs/{run_id}/`
//!
//! 3. **Immutability**: Same `bundle_id` = same bytes. No versioning needed.
//!    Enforced via conditional writes (`PutMode::Create`).
//!
//! 4. **No date folders**: Lifecycle policies can use object metadata/tags.
//!    Simpler key structure = fewer list/pull mismatch bugs.

use object_store::path::Path;

use super::error::{StoreError, StoreResult};

/// Builder for storage keys.
#[derive(Debug, Clone)]
pub struct KeyBuilder {
    /// Base prefix (e.g., "assay/evidence")
    base_prefix: String,
}

impl KeyBuilder {
    /// Create a new key builder with the given base prefix.
    pub fn new(base_prefix: impl Into<String>) -> Self {
        let mut prefix = base_prefix.into();
        // Normalize: remove leading/trailing slashes
        prefix = prefix.trim_matches('/').to_string();
        Self {
            base_prefix: prefix,
        }
    }

    /// Validate an identifier (such as `run_id` or `bundle_id`) before key construction.
    ///
    /// Refuses:
    /// - empty strings
    /// - forward slash `/`
    /// - path traversal `..`
    /// - control characters
    pub fn validate_id(id: &str) -> StoreResult<()> {
        if id.is_empty() {
            return Err(StoreError::InvalidId {
                id: id.to_string(),
                reason: "identifier must not be empty".to_string(),
            });
        }
        if id.contains('/') {
            return Err(StoreError::InvalidId {
                id: id.to_string(),
                reason: "identifier must not contain '/'".to_string(),
            });
        }
        if id.contains("..") {
            return Err(StoreError::InvalidId {
                id: id.to_string(),
                reason: "identifier must not contain path traversal '..'".to_string(),
            });
        }
        if id.chars().any(|c| c.is_control()) {
            return Err(StoreError::InvalidId {
                id: id.to_string(),
                reason: "identifier must not contain control characters".to_string(),
            });
        }
        Ok(())
    }

    /// Key for the bundle tarball.
    ///
    /// Returns: `{base}/bundles/{bundle_id}.tar.gz`
    pub fn bundle_key(&self, bundle_id: &str) -> StoreResult<Path> {
        Self::validate_id(bundle_id)?;
        let sanitized = Self::sanitize_id(bundle_id);
        if self.base_prefix.is_empty() {
            Ok(Path::from(format!("bundles/{}.tar.gz", sanitized)))
        } else {
            Ok(Path::from(format!(
                "{}/bundles/{}.tar.gz",
                self.base_prefix, sanitized
            )))
        }
    }

    /// Prefix for listing all bundles.
    ///
    /// Returns: `{base}/bundles/`
    pub fn bundles_prefix(&self) -> Path {
        if self.base_prefix.is_empty() {
            Path::from("bundles/")
        } else {
            Path::from(format!("{}/bundles/", self.base_prefix))
        }
    }

    /// Probe key used by store-status write checks.
    ///
    /// Returns: `{base}/.assay_probe_write_test`
    pub fn probe_key(&self) -> Path {
        if self.base_prefix.is_empty() {
            Path::from(".assay_probe_write_test")
        } else {
            Path::from(format!("{}/.assay_probe_write_test", self.base_prefix))
        }
    }

    /// Key for a run-to-bundle reference.
    ///
    /// Returns: `{base}/runs/{run_id}/{bundle_id}.ref`
    pub fn run_bundle_ref_key(&self, run_id: &str, bundle_id: &str) -> StoreResult<Path> {
        Self::validate_id(run_id)?;
        Self::validate_id(bundle_id)?;
        let run_id = Self::sanitize_id(run_id);
        let bundle_id = Self::sanitize_id(bundle_id);
        if self.base_prefix.is_empty() {
            Ok(Path::from(format!("runs/{}/{}.ref", run_id, bundle_id)))
        } else {
            Ok(Path::from(format!(
                "{}/runs/{}/{}.ref",
                self.base_prefix, run_id, bundle_id
            )))
        }
    }

    /// Prefix for listing bundles in a run.
    ///
    /// Returns: `{base}/runs/{run_id}/`
    pub fn run_bundles_prefix(&self, run_id: &str) -> StoreResult<Path> {
        Self::validate_id(run_id)?;
        let run_id = Self::sanitize_id(run_id);
        if self.base_prefix.is_empty() {
            Ok(Path::from(format!("runs/{}/", run_id)))
        } else {
            Ok(Path::from(format!("{}/runs/{}/", self.base_prefix, run_id)))
        }
    }

    /// Prefix for listing all run references.
    ///
    /// Returns: `{base}/runs/`
    pub fn runs_prefix(&self) -> Path {
        if self.base_prefix.is_empty() {
            Path::from("runs/")
        } else {
            Path::from(format!("{}/runs/", self.base_prefix))
        }
    }

    /// Extract bundle_id from a bundle key.
    ///
    /// Input: `{base}/bundles/{bundle_id}.tar.gz`
    /// Output: `Some(bundle_id)`
    pub fn parse_bundle_key(&self, key: &Path) -> Option<String> {
        let key_str = key.as_ref();

        // Look for /bundles/{id}.tar.gz or bundles/{id}.tar.gz pattern
        if !key_str.ends_with(".tar.gz") {
            return None;
        }

        let parts: Vec<&str> = key_str.split('/').collect();

        // Find "bundles" segment and extract the next part (the bundle_id.tar.gz)
        for (i, part) in parts.iter().enumerate() {
            if *part == "bundles" && i + 1 < parts.len() {
                let filename = parts[i + 1];
                if let Some(bundle_id) = filename.strip_suffix(".tar.gz") {
                    if Self::validate_id(bundle_id).is_ok() {
                        return Some(bundle_id.to_string());
                    }
                }
            }
        }
        None
    }

    /// Extract bundle_id from a run reference key.
    ///
    /// Input: `{base}/runs/{run_id}/{bundle_id}.ref`
    /// Output: `Some(bundle_id)`
    pub fn parse_run_ref_key(&self, key: &Path) -> Option<String> {
        let key_str = key.as_ref();
        // Look for .ref extension and extract filename without it
        if !key_str.ends_with(".ref") {
            return None;
        }

        key_str
            .rsplit('/')
            .next()
            .and_then(|filename| filename.strip_suffix(".ref"))
            .filter(|bundle_id| Self::validate_id(bundle_id).is_ok())
            .map(|s| s.to_string())
    }

    /// Extract `(run_id, bundle_id)` from a run reference key.
    ///
    /// Accepts `{base}/runs/{run_id}/{bundle_id}.ref`.
    pub fn parse_run_ref_parts(&self, key: &Path) -> Option<(String, String)> {
        let key_str = key.as_ref();
        if !key_str.ends_with(".ref") {
            return None;
        }
        let parts: Vec<&str> = key_str.split('/').collect();
        for (i, part) in parts.iter().enumerate() {
            if *part == "runs" && i + 2 < parts.len() {
                // Must have exactly two segments after "runs": run_id and bundle_id.ref
                if parts.len() != i + 3 {
                    return None;
                }
                let run_id = parts[i + 1].to_string();
                let bundle_id = parts[i + 2].strip_suffix(".ref")?.to_string();
                if Self::validate_id(&run_id).is_err() || Self::validate_id(&bundle_id).is_err() {
                    return None;
                }
                return Some((run_id, bundle_id));
            }
        }
        None
    }

    /// Sanitize an ID for use in keys.
    /// Replaces potentially problematic characters.
    fn sanitize_id(id: &str) -> String {
        // Allow alphanumeric, dash, underscore, colon (for sha256:...)
        id.chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' || c == ':' || c == '.' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundle_key() {
        let kb = KeyBuilder::new("assay/evidence");
        let key = kb.bundle_key("sha256:abc123").unwrap();
        assert_eq!(key.as_ref(), "assay/evidence/bundles/sha256:abc123.tar.gz");
    }

    #[test]
    fn test_bundle_key_no_prefix() {
        let kb = KeyBuilder::new("");
        let key = kb.bundle_key("sha256:abc123").unwrap();
        assert_eq!(key.as_ref(), "bundles/sha256:abc123.tar.gz");
    }

    #[test]
    fn test_run_bundle_ref_key() {
        let kb = KeyBuilder::new("assay");
        let key = kb.run_bundle_ref_key("run_001", "sha256:abc123").unwrap();
        assert_eq!(key.as_ref(), "assay/runs/run_001/sha256:abc123.ref");
    }

    #[test]
    fn test_parse_bundle_key() {
        let kb = KeyBuilder::new("assay/evidence");
        let key = Path::from("assay/evidence/bundles/sha256:abc123.tar.gz");
        assert_eq!(kb.parse_bundle_key(&key), Some("sha256:abc123".to_string()));
    }

    #[test]
    fn test_parse_run_ref_key() {
        let kb = KeyBuilder::new("assay");
        let key = Path::from("assay/runs/run_001/sha256:abc123.ref");
        assert_eq!(
            kb.parse_run_ref_key(&key),
            Some("sha256:abc123".to_string())
        );
    }

    #[test]
    fn test_bundles_prefix() {
        let kb = KeyBuilder::new("assay");
        // Note: object_store::Path normalizes trailing slashes
        assert!(kb.bundles_prefix().as_ref().starts_with("assay/bundles"));
    }

    #[test]
    fn test_run_bundles_prefix() {
        let kb = KeyBuilder::new("assay");
        // Note: object_store::Path normalizes trailing slashes
        assert!(kb
            .run_bundles_prefix("run_001")
            .unwrap()
            .as_ref()
            .starts_with("assay/runs/run_001"));
    }

    #[test]
    fn test_runs_prefix() {
        let kb = KeyBuilder::new("assay");
        assert!(kb.runs_prefix().as_ref().starts_with("assay/runs"));

        let kb_empty = KeyBuilder::new("");
        assert_eq!(kb_empty.runs_prefix().as_ref(), "runs");
    }

    #[test]
    fn test_parse_run_ref_parts() {
        let kb = KeyBuilder::new("assay");
        let k1 = Path::from("assay/runs/run_001/sha256:abc123.ref");
        assert_eq!(
            kb.parse_run_ref_parts(&k1),
            Some(("run_001".to_string(), "sha256:abc123".to_string()))
        );

        let k3 = Path::from("runs/run_002/sha256:def456.ref");
        assert_eq!(
            kb.parse_run_ref_parts(&k3),
            Some(("run_002".to_string(), "sha256:def456".to_string()))
        );

        let not_ref = Path::from("assay/runs/run_001/sha256:abc123.txt");
        assert_eq!(kb.parse_run_ref_parts(&not_ref), None);

        let bundle_key = Path::from("assay/bundles/sha256:abc123.tar.gz");
        assert_eq!(kb.parse_run_ref_parts(&bundle_key), None);
    }

    #[test]
    fn test_validate_id_refuses_empty() {
        assert!(KeyBuilder::validate_id("").is_err());
        let err = KeyBuilder::validate_id("").unwrap_err();
        match err {
            StoreError::InvalidId { id, reason } => {
                assert_eq!(id, "");
                assert!(reason.contains("empty"));
            }
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn test_validate_id_refuses_slash() {
        assert!(KeyBuilder::validate_id("foo/bar").is_err());
        let err = KeyBuilder::validate_id("foo/bar").unwrap_err();
        match err {
            StoreError::InvalidId { id, reason } => {
                assert_eq!(id, "foo/bar");
                assert!(reason.contains('/'));
            }
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn test_validate_id_refuses_path_traversal() {
        assert!(KeyBuilder::validate_id("..").is_err());
        assert!(KeyBuilder::validate_id("a/../b").is_err());
        assert!(KeyBuilder::validate_id("foo..bar").is_err());
        let err = KeyBuilder::validate_id("..").unwrap_err();
        match err {
            StoreError::InvalidId { id, reason } => {
                assert_eq!(id, "..");
                assert!(reason.contains(".."));
            }
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn test_validate_id_refuses_control_characters() {
        assert!(KeyBuilder::validate_id("foo\0bar").is_err());
        assert!(KeyBuilder::validate_id("foo\nbar").is_err());
        assert!(KeyBuilder::validate_id("foo\rbar").is_err());
        assert!(KeyBuilder::validate_id("\x1b[31m").is_err());
        let err = KeyBuilder::validate_id("foo\0bar").unwrap_err();
        match err {
            StoreError::InvalidId { reason, .. } => {
                assert!(reason.contains("control"));
            }
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn test_key_builder_refuses_empty_and_hostile_ids() {
        let kb = KeyBuilder::new("assay");

        // Empty ID cannot produce runs/.ref
        assert!(kb.run_bundle_ref_key("", "").is_err());
        assert!(kb.run_bundle_ref_key("run1", "").is_err());
        assert!(kb.run_bundle_ref_key("", "bundle1").is_err());
        assert!(kb.bundle_key("").is_err());
        assert!(kb.run_bundles_prefix("").is_err());

        // Hostile IDs refused
        assert!(kb.run_bundle_ref_key("../escaped", "bundle").is_err());
        assert!(kb.run_bundle_ref_key("run", "bundle/slash").is_err());
        assert!(kb.run_bundle_ref_key("run\0null", "bundle").is_err());
    }

    #[test]
    fn test_parse_run_ref_parts_rejects_empty_ids() {
        let kb = KeyBuilder::new("assay");
        // Key like runs/.ref has empty bundle_id
        let empty_ref = Path::from("runs/.ref");
        assert_eq!(kb.parse_run_ref_parts(&empty_ref), None);

        let empty_run = Path::from("runs//bundle.ref");
        assert_eq!(kb.parse_run_ref_parts(&empty_run), None);
    }

    #[test]
    fn test_probe_key() {
        let kb = KeyBuilder::new("assay/evidence");
        assert_eq!(
            kb.probe_key().as_ref(),
            "assay/evidence/.assay_probe_write_test"
        );
    }

    #[test]
    fn test_probe_key_no_prefix() {
        let kb = KeyBuilder::new("");
        assert_eq!(kb.probe_key().as_ref(), ".assay_probe_write_test");
    }
}
