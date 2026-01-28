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
    /// Returns: `{base}/bundles/{bundle_id}.tar.gz`
    pub fn bundle_key(&self, bundle_id: &str) -> Path {
        let sanitized = Self::sanitize_id(bundle_id);
        if self.base_prefix.is_empty() {
            Path::from(format!("bundles/{}.tar.gz", sanitized))
        } else {
            Path::from(format!("{}/bundles/{}.tar.gz", self.base_prefix, sanitized))
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
    /// Returns: `{base}/runs/{run_id}/{bundle_id}.ref`
    pub fn run_bundle_ref_key(&self, run_id: &str, bundle_id: &str) -> Path {
        let run_id = Self::sanitize_id(run_id);
        let bundle_id = Self::sanitize_id(bundle_id);
        if self.base_prefix.is_empty() {
            Path::from(format!("runs/{}/{}.ref", run_id, bundle_id))
        } else {
            Path::from(format!(
                "{}/runs/{}/{}.ref",
                self.base_prefix, run_id, bundle_id
            ))
        }
    }

    /// Prefix for listing bundles in a run.
    ///
    /// Returns: `{base}/runs/{run_id}/`
    pub fn run_bundles_prefix(&self, run_id: &str) -> Path {
        let run_id = Self::sanitize_id(run_id);
        if self.base_prefix.is_empty() {
            Path::from(format!("runs/{}/", run_id))
        } else {
            Path::from(format!("{}/runs/{}/", self.base_prefix, run_id))
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
                return filename.strip_suffix(".tar.gz").map(|s| s.to_string());
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
        assert_eq!(key.as_ref(), "assay/evidence/bundles/sha256:abc123.tar.gz");
    }

    #[test]
    fn test_bundle_key_no_prefix() {
        let kb = KeyBuilder::new("");
        let key = kb.bundle_key("sha256:abc123");
        assert_eq!(key.as_ref(), "bundles/sha256:abc123.tar.gz");
    }

    #[test]
    fn test_run_bundle_ref_key() {
        let kb = KeyBuilder::new("assay");
        let key = kb.run_bundle_ref_key("run_001", "sha256:abc123");
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
            .as_ref()
            .starts_with("assay/runs/run_001"));
    }
}
