//! Pack reference parsing.
//!
//! Supports various reference formats:
//! - `./custom.yaml` → local file
//! - `eu-ai-act-baseline` → bundled pack
//! - `eu-ai-act-pro@1.2.0` → registry pack
//! - `eu-ai-act-pro@1.2.0#sha256:abc...` → registry pack with pinned digest
//! - `s3://bucket/pack.yaml` → BYOS (Bring Your Own Storage)

use std::path::PathBuf;

use crate::error::{RegistryError, RegistryResult};

/// A parsed pack reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackRef {
    /// Local file path (relative or absolute).
    Local(PathBuf),

    /// Bundled pack (name only, no version).
    Bundled(String),

    /// Registry pack with version.
    Registry {
        name: String,
        version: String,
        /// Optional pinned digest (sha256:...).
        pinned_digest: Option<String>,
    },

    /// BYOS (Bring Your Own Storage) URL.
    Byos(String),
}

impl PackRef {
    /// Parse a pack reference string.
    ///
    /// # Examples
    ///
    /// ```
    /// use assay_registry::PackRef;
    ///
    /// // Local file
    /// let local = PackRef::parse("./custom.yaml").unwrap();
    /// assert!(matches!(local, PackRef::Local(_)));
    ///
    /// // Bundled pack
    /// let bundled = PackRef::parse("eu-ai-act-baseline").unwrap();
    /// assert!(matches!(bundled, PackRef::Bundled(_)));
    ///
    /// // Registry pack with version
    /// let registry = PackRef::parse("eu-ai-act-pro@1.2.0").unwrap();
    /// assert!(matches!(registry, PackRef::Registry { .. }));
    ///
    /// // Registry pack with pinned digest
    /// let pinned = PackRef::parse("eu-ai-act-pro@1.2.0#sha256:abc123").unwrap();
    /// if let PackRef::Registry { pinned_digest, .. } = pinned {
    ///     assert!(pinned_digest.is_some());
    /// }
    ///
    /// // BYOS
    /// let byos = PackRef::parse("s3://bucket/pack.yaml").unwrap();
    /// assert!(matches!(byos, PackRef::Byos(_)));
    /// ```
    pub fn parse(reference: &str) -> RegistryResult<Self> {
        let reference = reference.trim();

        if reference.is_empty() {
            return Err(RegistryError::InvalidReference {
                reference: reference.to_string(),
                reason: "empty reference".to_string(),
            });
        }

        // Check for BYOS URLs first (s3://, gs://, azure://, https://)
        if reference.starts_with("s3://")
            || reference.starts_with("gs://")
            || reference.starts_with("azure://")
            || reference.starts_with("https://")
            || reference.starts_with("http://")
        {
            return Ok(Self::Byos(reference.to_string()));
        }

        // Check for local file paths
        if reference.starts_with("./")
            || reference.starts_with("../")
            || reference.starts_with('/')
            || reference.ends_with(".yaml")
            || reference.ends_with(".yml")
        {
            return Ok(Self::Local(PathBuf::from(reference)));
        }

        // Check for Windows absolute paths
        if reference.len() >= 2 && reference.chars().nth(1) == Some(':') {
            return Ok(Self::Local(PathBuf::from(reference)));
        }

        // Check for registry reference (name@version#digest)
        if let Some(at_pos) = reference.find('@') {
            let name = &reference[..at_pos];
            let rest = &reference[at_pos + 1..];

            // Check for pinned digest
            let (version, pinned_digest) = if let Some(hash_pos) = rest.find('#') {
                let version = &rest[..hash_pos];
                let digest = &rest[hash_pos + 1..];

                // Validate digest format
                if !digest.starts_with("sha256:") {
                    return Err(RegistryError::InvalidReference {
                        reference: reference.to_string(),
                        reason: "pinned digest must start with 'sha256:'".to_string(),
                    });
                }

                (version.to_string(), Some(digest.to_string()))
            } else {
                (rest.to_string(), None)
            };

            // Validate name
            validate_pack_name(name)?;

            // Validate version is not empty
            if version.is_empty() {
                return Err(RegistryError::InvalidReference {
                    reference: reference.to_string(),
                    reason: "version is required for registry packs".to_string(),
                });
            }

            return Ok(Self::Registry {
                name: name.to_string(),
                version,
                pinned_digest,
            });
        }

        // Assume bundled pack (name only)
        validate_pack_name(reference)?;
        Ok(Self::Bundled(reference.to_string()))
    }

    /// Check if this is a local file reference.
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local(_))
    }

    /// Check if this is a bundled pack reference.
    pub fn is_bundled(&self) -> bool {
        matches!(self, Self::Bundled(_))
    }

    /// Check if this is a registry pack reference.
    pub fn is_registry(&self) -> bool {
        matches!(self, Self::Registry { .. })
    }

    /// Check if this is a BYOS reference.
    pub fn is_byos(&self) -> bool {
        matches!(self, Self::Byos(_))
    }

    /// Get the pack name (for bundled and registry refs).
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Bundled(name) => Some(name),
            Self::Registry { name, .. } => Some(name),
            _ => None,
        }
    }

    /// Get the version (for registry refs).
    pub fn version(&self) -> Option<&str> {
        match self {
            Self::Registry { version, .. } => Some(version),
            _ => None,
        }
    }

    /// Get the pinned digest (for registry refs).
    pub fn pinned_digest(&self) -> Option<&str> {
        match self {
            Self::Registry { pinned_digest, .. } => pinned_digest.as_deref(),
            _ => None,
        }
    }
}

impl std::fmt::Display for PackRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local(path) => write!(f, "{}", path.display()),
            Self::Bundled(name) => write!(f, "{}", name),
            Self::Registry {
                name,
                version,
                pinned_digest: None,
            } => write!(f, "{}@{}", name, version),
            Self::Registry {
                name,
                version,
                pinned_digest: Some(digest),
            } => write!(f, "{}@{}#{}", name, version, digest),
            Self::Byos(url) => write!(f, "{}", url),
        }
    }
}

impl std::str::FromStr for PackRef {
    type Err = RegistryError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

/// Validate a pack name.
fn validate_pack_name(name: &str) -> RegistryResult<()> {
    if name.is_empty() {
        return Err(RegistryError::InvalidReference {
            reference: name.to_string(),
            reason: "pack name cannot be empty".to_string(),
        });
    }

    // Must start with lowercase letter
    if !name
        .chars()
        .next()
        .map(|c| c.is_ascii_lowercase())
        .unwrap_or(false)
    {
        return Err(RegistryError::InvalidReference {
            reference: name.to_string(),
            reason: "pack name must start with a lowercase letter".to_string(),
        });
    }

    // Must only contain lowercase letters, digits, and hyphens
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(RegistryError::InvalidReference {
            reference: name.to_string(),
            reason: "pack name may only contain lowercase letters, digits, and hyphens".to_string(),
        });
    }

    // Cannot end with hyphen
    if name.ends_with('-') {
        return Err(RegistryError::InvalidReference {
            reference: name.to_string(),
            reason: "pack name cannot end with a hyphen".to_string(),
        });
    }

    // Cannot have consecutive hyphens
    if name.contains("--") {
        return Err(RegistryError::InvalidReference {
            reference: name.to_string(),
            reason: "pack name cannot have consecutive hyphens".to_string(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_local_relative() {
        let pack_ref = PackRef::parse("./custom.yaml").unwrap();
        assert!(
            matches!(pack_ref, PackRef::Local(p) if p.as_path() == std::path::Path::new("./custom.yaml"))
        );
    }

    #[test]
    fn test_parse_local_parent() {
        let pack_ref = PackRef::parse("../packs/custom.yaml").unwrap();
        assert!(matches!(pack_ref, PackRef::Local(_)));
    }

    #[test]
    fn test_parse_local_absolute() {
        let pack_ref = PackRef::parse("/home/user/packs/custom.yaml").unwrap();
        assert!(matches!(pack_ref, PackRef::Local(_)));
    }

    #[test]
    fn test_parse_local_by_extension() {
        let pack_ref = PackRef::parse("custom.yaml").unwrap();
        assert!(matches!(pack_ref, PackRef::Local(_)));
    }

    #[test]
    fn test_parse_bundled() {
        let pack_ref = PackRef::parse("eu-ai-act-baseline").unwrap();
        assert_eq!(pack_ref, PackRef::Bundled("eu-ai-act-baseline".to_string()));
    }

    #[test]
    fn test_parse_registry() {
        let pack_ref = PackRef::parse("eu-ai-act-pro@1.2.0").unwrap();
        assert_eq!(
            pack_ref,
            PackRef::Registry {
                name: "eu-ai-act-pro".to_string(),
                version: "1.2.0".to_string(),
                pinned_digest: None,
            }
        );
    }

    #[test]
    fn test_parse_registry_with_digest() {
        let pack_ref = PackRef::parse("eu-ai-act-pro@1.2.0#sha256:abc123").unwrap();
        assert_eq!(
            pack_ref,
            PackRef::Registry {
                name: "eu-ai-act-pro".to_string(),
                version: "1.2.0".to_string(),
                pinned_digest: Some("sha256:abc123".to_string()),
            }
        );
    }

    #[test]
    fn test_parse_byos_s3() {
        let pack_ref = PackRef::parse("s3://bucket/path/pack.yaml").unwrap();
        assert_eq!(
            pack_ref,
            PackRef::Byos("s3://bucket/path/pack.yaml".to_string())
        );
    }

    #[test]
    fn test_parse_byos_https() {
        let pack_ref = PackRef::parse("https://example.com/packs/custom.yaml").unwrap();
        assert_eq!(
            pack_ref,
            PackRef::Byos("https://example.com/packs/custom.yaml".to_string())
        );
    }

    #[test]
    fn test_parse_empty() {
        let result = PackRef::parse("");
        assert!(matches!(
            result,
            Err(RegistryError::InvalidReference { .. })
        ));
    }

    #[test]
    fn test_parse_invalid_digest() {
        let result = PackRef::parse("pack@1.0.0#md5:abc123");
        assert!(matches!(
            result,
            Err(RegistryError::InvalidReference { .. })
        ));
    }

    #[test]
    fn test_parse_missing_version() {
        let result = PackRef::parse("pack@");
        assert!(matches!(
            result,
            Err(RegistryError::InvalidReference { .. })
        ));
    }

    #[test]
    fn test_validate_name_uppercase() {
        let result = validate_pack_name("MyPack");
        assert!(matches!(
            result,
            Err(RegistryError::InvalidReference { .. })
        ));
    }

    #[test]
    fn test_validate_name_starts_with_digit() {
        let result = validate_pack_name("123-pack");
        assert!(matches!(
            result,
            Err(RegistryError::InvalidReference { .. })
        ));
    }

    #[test]
    fn test_validate_name_ends_with_hyphen() {
        let result = validate_pack_name("pack-");
        assert!(matches!(
            result,
            Err(RegistryError::InvalidReference { .. })
        ));
    }

    #[test]
    fn test_validate_name_consecutive_hyphens() {
        let result = validate_pack_name("pack--name");
        assert!(matches!(
            result,
            Err(RegistryError::InvalidReference { .. })
        ));
    }

    #[test]
    fn test_display() {
        assert_eq!(
            PackRef::Local(PathBuf::from("./custom.yaml")).to_string(),
            "./custom.yaml"
        );
        assert_eq!(
            PackRef::Bundled("my-pack".to_string()).to_string(),
            "my-pack"
        );
        assert_eq!(
            PackRef::Registry {
                name: "pack".to_string(),
                version: "1.0.0".to_string(),
                pinned_digest: None
            }
            .to_string(),
            "pack@1.0.0"
        );
        assert_eq!(
            PackRef::Registry {
                name: "pack".to_string(),
                version: "1.0.0".to_string(),
                pinned_digest: Some("sha256:abc".to_string())
            }
            .to_string(),
            "pack@1.0.0#sha256:abc"
        );
    }

    #[test]
    fn test_accessors() {
        let registry_ref = PackRef::Registry {
            name: "my-pack".to_string(),
            version: "1.0.0".to_string(),
            pinned_digest: Some("sha256:abc".to_string()),
        };

        assert!(registry_ref.is_registry());
        assert!(!registry_ref.is_local());
        assert!(!registry_ref.is_bundled());
        assert!(!registry_ref.is_byos());
        assert_eq!(registry_ref.name(), Some("my-pack"));
        assert_eq!(registry_ref.version(), Some("1.0.0"));
        assert_eq!(registry_ref.pinned_digest(), Some("sha256:abc"));
    }

    #[test]
    fn test_from_str() {
        let pack_ref: PackRef = "eu-ai-act-pro@1.2.0".parse().unwrap();
        assert!(matches!(pack_ref, PackRef::Registry { .. }));
    }
}
