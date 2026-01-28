//! Key naming conventions for bundle storage.
//!
//! # Key Schema
//!
//! ```text
//! {base_prefix}/bundles/{bundle_id}/bundle.tar.gz
//! {base_prefix}/runs/{run_id}/bundles/{bundle_id}.ref
//! ```
//!
//! ## Why this structure?
//!
//! 1. **Content-addressed prefix**: `bundles/{bundle_id}/` distributes keys
//!    well across S3 partitions (hash randomizes early in key).
//!
//! 2. **Immutability**: Same `bundle_id` = same bytes. No versioning needed.
//!
//! 3. **Run indexing**: `runs/{run_id}/bundles/` enables `list --run-id`
//!    via simple prefix listing, without a database.
//!
//! 4. **Future-proof**: Structure allows adding metadata files like
//!    `bundles/{id}/manifest.json` or `bundles/{id}/signature.json` later.

use object_store::path::Path;

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

    /// Key for the bundle tarball.
    ///
    /// Returns: `{base}/bundles/{bundle_id}/bundle.tar.gz`
    pub fn bundle_key(&self, bundle_id: &str) -> Path {
        let sanitized = Self::sanitize_id(bundle_id);
        if self.base_prefix.is_empty() {
            Path::from(format!("bundles/{}/bundle.tar.gz", sanitized))
        } else {
            Path::from(format!(
                "{}/bundles/{}/bundle.tar.gz",
                self.base_prefix, sanitized
            ))
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

    /// Key for a run-to-bundle reference.
    ///
    /// Returns: `{base}/runs/{run_id}/bundles/{bundle_id}.ref`
    pub fn run_bundle_ref_key(&self, run_id: &str, bundle_id: &str) -> Path {
        let run_id = Self::sanitize_id(run_id);
        let bundle_id = Self::sanitize_id(bundle_id);
        if self.base_prefix.is_empty() {
            Path::from(format!("runs/{}/bundles/{}.ref", run_id, bundle_id))
        } else {
            Path::from(format!(
                "{}/runs/{}/bundles/{}.ref",
                self.base_prefix, run_id, bundle_id
            ))
        }
    }

    /// Prefix for listing bundles in a run.
    ///
    /// Returns: `{base}/runs/{run_id}/bundles/`
    pub fn run_bundles_prefix(&self, run_id: &str) -> Path {
        let run_id = Self::sanitize_id(run_id);
        if self.base_prefix.is_empty() {
            Path::from(format!("runs/{}/bundles/", run_id))
        } else {
            Path::from(format!("{}/runs/{}/bundles/", self.base_prefix, run_id))
        }
    }

    /// Extract bundle_id from a bundle key.
    ///
    /// Input: `{base}/bundles/{bundle_id}/bundle.tar.gz`
    /// Output: `Some(bundle_id)`
    pub fn parse_bundle_key(&self, key: &Path) -> Option<String> {
        let key_str = key.as_ref();
        // Look for /bundles/{id}/bundle.tar.gz pattern
        let parts: Vec<&str> = key_str.split('/').collect();

        // Find "bundles" segment and extract the next part
        for (i, part) in parts.iter().enumerate() {
            if *part == "bundles" && i + 2 < parts.len() && parts[i + 2] == "bundle.tar.gz" {
                return Some(parts[i + 1].to_string());
            }
        }
        None
    }

    /// Extract bundle_id from a run reference key.
    ///
    /// Input: `{base}/runs/{run_id}/bundles/{bundle_id}.ref`
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
            .map(|s| s.to_string())
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
        let key = kb.bundle_key("sha256:abc123");
        assert_eq!(
            key.as_ref(),
            "assay/evidence/bundles/sha256:abc123/bundle.tar.gz"
        );
    }

    #[test]
    fn test_bundle_key_no_prefix() {
        let kb = KeyBuilder::new("");
        let key = kb.bundle_key("sha256:abc123");
        assert_eq!(key.as_ref(), "bundles/sha256:abc123/bundle.tar.gz");
    }

    #[test]
    fn test_run_bundle_ref_key() {
        let kb = KeyBuilder::new("assay");
        let key = kb.run_bundle_ref_key("run_001", "sha256:abc123");
        assert_eq!(key.as_ref(), "assay/runs/run_001/bundles/sha256:abc123.ref");
    }

    #[test]
    fn test_parse_bundle_key() {
        let kb = KeyBuilder::new("assay/evidence");
        let key = Path::from("assay/evidence/bundles/sha256:abc123/bundle.tar.gz");
        assert_eq!(kb.parse_bundle_key(&key), Some("sha256:abc123".to_string()));
    }

    #[test]
    fn test_parse_run_ref_key() {
        let kb = KeyBuilder::new("assay");
        let key = Path::from("assay/runs/run_001/bundles/sha256:abc123.ref");
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
            .as_ref()
            .starts_with("assay/runs/run_001/bundles"));
    }
}
