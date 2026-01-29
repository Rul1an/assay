//! Lockfile support for reproducible builds.
//!
//! The lockfile (`assay.packs.lock`) records exact pack versions and digests
//! to ensure reproducible builds across machines and CI runs.
//!
//! # Lockfile Format (v2)
//!
//! ```yaml
//! version: 2
//! generated_at: "2026-01-29T10:00:00Z"
//! generated_by: "assay-cli/2.10.1"
//! packs:
//!   - name: eu-ai-act-pro
//!     version: "1.2.0"
//!     digest: sha256:abc123...
//!     source: registry
//!     registry_url: "https://registry.getassay.dev/v1"
//!     signature:
//!       algorithm: Ed25519
//!       key_id: sha256:def456...
//! ```

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::{debug, info, warn};

use crate::error::{RegistryError, RegistryResult};
use crate::reference::PackRef;
use crate::resolver::{PackResolver, ResolveSource};

/// Default lockfile name.
pub const LOCKFILE_NAME: &str = "assay.packs.lock";

/// Current lockfile schema version.
pub const LOCKFILE_VERSION: u8 = 2;

/// A pack lockfile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lockfile {
    /// Schema version.
    pub version: u8,

    /// When the lockfile was generated.
    pub generated_at: DateTime<Utc>,

    /// Tool that generated the lockfile.
    pub generated_by: String,

    /// Locked packs.
    #[serde(default)]
    pub packs: Vec<LockedPack>,
}

/// A locked pack entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LockedPack {
    /// Pack name.
    pub name: String,

    /// Pack version.
    pub version: String,

    /// Content digest (sha256:...).
    pub digest: String,

    /// Source type.
    pub source: LockSource,

    /// Registry URL (if source is registry).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry_url: Option<String>,

    /// BYOS URL (if source is byos).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byos_url: Option<String>,

    /// Signature information (if signed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<LockSignature>,
}

/// Source type for locked packs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LockSource {
    /// Bundled pack.
    Bundled,

    /// Registry pack.
    Registry,

    /// BYOS pack.
    Byos,

    /// Local file (not recommended for lockfiles).
    Local,
}

/// Signature information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LockSignature {
    /// Signature algorithm.
    pub algorithm: String,

    /// Key ID used for signing.
    pub key_id: String,
}

/// Lockfile verification result.
#[derive(Debug, Clone)]
pub struct VerifyLockResult {
    /// Whether all packs match.
    pub all_match: bool,

    /// Packs that matched.
    pub matched: Vec<String>,

    /// Packs with digest mismatches.
    pub mismatched: Vec<LockMismatch>,

    /// Packs in lockfile but not resolved.
    pub missing: Vec<String>,

    /// Packs resolved but not in lockfile.
    pub extra: Vec<String>,
}

/// A lockfile mismatch.
#[derive(Debug, Clone)]
pub struct LockMismatch {
    /// Pack name.
    pub name: String,

    /// Pack version.
    pub version: String,

    /// Expected digest from lockfile.
    pub expected: String,

    /// Actual digest from resolution.
    pub actual: String,
}

impl Lockfile {
    /// Create a new empty lockfile.
    pub fn new() -> Self {
        Self {
            version: LOCKFILE_VERSION,
            generated_at: Utc::now(),
            generated_by: format!("assay-cli/{}", env!("CARGO_PKG_VERSION")),
            packs: Vec::new(),
        }
    }

    /// Load a lockfile from a path.
    pub async fn load(path: impl AsRef<Path>) -> RegistryResult<Self> {
        let path = path.as_ref();

        if !path.exists() {
            return Err(RegistryError::Lockfile {
                message: format!("lockfile not found: {}", path.display()),
            });
        }

        let content = fs::read_to_string(path)
            .await
            .map_err(|e| RegistryError::Lockfile {
                message: format!("failed to read lockfile: {}", e),
            })?;

        Self::parse(&content)
    }

    /// Parse a lockfile from YAML content.
    pub fn parse(content: &str) -> RegistryResult<Self> {
        let lockfile: Lockfile =
            serde_yaml::from_str(content).map_err(|e| RegistryError::Lockfile {
                message: format!("failed to parse lockfile: {}", e),
            })?;

        // Validate version
        if lockfile.version > LOCKFILE_VERSION {
            return Err(RegistryError::Lockfile {
                message: format!(
                    "lockfile version {} is newer than supported version {}",
                    lockfile.version, LOCKFILE_VERSION
                ),
            });
        }

        Ok(lockfile)
    }

    /// Save the lockfile to a path.
    pub async fn save(&self, path: impl AsRef<Path>) -> RegistryResult<()> {
        let path = path.as_ref();
        let content = self.to_yaml()?;

        fs::write(path, content)
            .await
            .map_err(|e| RegistryError::Lockfile {
                message: format!("failed to write lockfile: {}", e),
            })?;

        info!(path = %path.display(), "saved lockfile");
        Ok(())
    }

    /// Convert to YAML string.
    pub fn to_yaml(&self) -> RegistryResult<String> {
        serde_yaml::to_string(self).map_err(|e| RegistryError::Lockfile {
            message: format!("failed to serialize lockfile: {}", e),
        })
    }

    /// Add or update a pack in the lockfile.
    pub fn add_pack(&mut self, pack: LockedPack) {
        // Remove existing entry with same name
        self.packs.retain(|p| p.name != pack.name);
        self.packs.push(pack);

        // Keep sorted by name
        self.packs.sort_by(|a, b| a.name.cmp(&b.name));

        // Update timestamp
        self.generated_at = Utc::now();
    }

    /// Remove a pack from the lockfile.
    pub fn remove_pack(&mut self, name: &str) -> bool {
        let len_before = self.packs.len();
        self.packs.retain(|p| p.name != name);
        self.packs.len() != len_before
    }

    /// Get a locked pack by name.
    pub fn get_pack(&self, name: &str) -> Option<&LockedPack> {
        self.packs.iter().find(|p| p.name == name)
    }

    /// Check if a pack is locked.
    pub fn contains(&self, name: &str) -> bool {
        self.packs.iter().any(|p| p.name == name)
    }

    /// Get all pack names.
    pub fn pack_names(&self) -> Vec<&str> {
        self.packs.iter().map(|p| p.name.as_str()).collect()
    }
}

impl Default for Lockfile {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a lockfile from pack references.
pub async fn generate_lockfile(
    references: &[String],
    resolver: &PackResolver,
) -> RegistryResult<Lockfile> {
    let mut lockfile = Lockfile::new();

    for reference in references {
        debug!(reference, "locking pack");

        let pack_ref = PackRef::parse(reference)?;
        let resolved = resolver.resolve_ref(&pack_ref).await?;

        let (name, version) = match &pack_ref {
            PackRef::Bundled(name) => (name.clone(), "bundled".to_string()),
            PackRef::Registry { name, version, .. } => (name.clone(), version.clone()),
            PackRef::Byos(url) => {
                // Extract name from URL
                let name = url
                    .rsplit('/')
                    .next()
                    .unwrap_or("unknown")
                    .trim_end_matches(".yaml")
                    .trim_end_matches(".yml")
                    .to_string();
                (name, "byos".to_string())
            }
            PackRef::Local(path) => {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                warn!(
                    path = %path.display(),
                    "locking local file - consider using registry or bundled packs instead"
                );
                (name, "local".to_string())
            }
        };

        let (source, registry_url, byos_url) = match &resolved.source {
            ResolveSource::Local(_) => (LockSource::Local, None, None),
            ResolveSource::Bundled(_) => (LockSource::Bundled, None, None),
            ResolveSource::Cache => (LockSource::Registry, None, None),
            ResolveSource::Registry(url) => (LockSource::Registry, Some(url.clone()), None),
            ResolveSource::Byos(url) => (LockSource::Byos, None, Some(url.clone())),
        };

        let signature = resolved.verification.as_ref().and_then(|v| {
            v.key_id.as_ref().map(|key_id| LockSignature {
                algorithm: "Ed25519".to_string(),
                key_id: key_id.clone(),
            })
        });

        let locked = LockedPack {
            name,
            version,
            digest: resolved.digest,
            source,
            registry_url,
            byos_url,
            signature,
        };

        lockfile.add_pack(locked);
    }

    Ok(lockfile)
}

/// Verify packs against a lockfile.
pub async fn verify_lockfile(
    lockfile: &Lockfile,
    resolver: &PackResolver,
) -> RegistryResult<VerifyLockResult> {
    let mut matched = Vec::new();
    let mut mismatched = Vec::new();
    let mut missing = Vec::new();

    for locked in &lockfile.packs {
        debug!(name = %locked.name, version = %locked.version, "verifying locked pack");

        // Build reference based on source
        let reference = match locked.source {
            LockSource::Bundled => locked.name.clone(),
            LockSource::Registry => {
                format!("{}@{}#{}", locked.name, locked.version, locked.digest)
            }
            LockSource::Byos => locked
                .byos_url
                .clone()
                .unwrap_or_else(|| locked.name.clone()),
            LockSource::Local => {
                warn!(
                    name = %locked.name,
                    "cannot verify local pack - skipping"
                );
                continue;
            }
        };

        match resolver.resolve(&reference).await {
            Ok(resolved) => {
                if resolved.digest == locked.digest {
                    matched.push(locked.name.clone());
                } else {
                    mismatched.push(LockMismatch {
                        name: locked.name.clone(),
                        version: locked.version.clone(),
                        expected: locked.digest.clone(),
                        actual: resolved.digest,
                    });
                }
            }
            Err(e) => {
                warn!(name = %locked.name, error = %e, "failed to resolve locked pack");
                missing.push(locked.name.clone());
            }
        }
    }

    let all_match = mismatched.is_empty() && missing.is_empty();

    Ok(VerifyLockResult {
        all_match,
        matched,
        mismatched,
        missing,
        extra: Vec::new(), // Would need resolved refs to compute
    })
}

/// Check if lockfile is outdated (any pack has newer version available).
pub async fn check_lockfile(
    lockfile: &Lockfile,
    resolver: &PackResolver,
) -> RegistryResult<Vec<LockMismatch>> {
    // For now, just verify digests match
    let result = verify_lockfile(lockfile, resolver).await?;

    if !result.all_match {
        return Err(RegistryError::Lockfile {
            message: format!(
                "lockfile verification failed: {} mismatched, {} missing",
                result.mismatched.len(),
                result.missing.len()
            ),
        });
    }

    Ok(result.mismatched)
}

/// Update a lockfile with latest versions.
pub async fn update_lockfile(
    lockfile: &mut Lockfile,
    resolver: &PackResolver,
) -> RegistryResult<Vec<String>> {
    let mut updated = Vec::new();

    for locked in &mut lockfile.packs {
        if locked.source != LockSource::Registry {
            continue;
        }

        debug!(name = %locked.name, version = %locked.version, "checking for updates");

        // Build reference without pinned digest to get latest
        let reference = format!("{}@{}", locked.name, locked.version);

        match resolver.resolve(&reference).await {
            Ok(resolved) => {
                if resolved.digest != locked.digest {
                    info!(
                        name = %locked.name,
                        old_digest = %locked.digest,
                        new_digest = %resolved.digest,
                        "updating locked digest"
                    );

                    locked.digest = resolved.digest;
                    updated.push(locked.name.clone());
                }
            }
            Err(e) => {
                warn!(name = %locked.name, error = %e, "failed to update pack");
            }
        }
    }

    if !updated.is_empty() {
        lockfile.generated_at = Utc::now();
    }

    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lockfile_new() {
        let lockfile = Lockfile::new();
        assert_eq!(lockfile.version, LOCKFILE_VERSION);
        assert!(lockfile.packs.is_empty());
    }

    #[test]
    fn test_lockfile_parse() {
        let yaml = r#"
version: 2
generated_at: "2026-01-29T10:00:00Z"
generated_by: "assay-cli/2.10.1"
packs:
  - name: eu-ai-act-pro
    version: "1.2.0"
    digest: sha256:abc123def456
    source: registry
    registry_url: "https://registry.getassay.dev/v1"
    signature:
      algorithm: Ed25519
      key_id: sha256:keyid123
"#;

        let lockfile = Lockfile::parse(yaml).unwrap();
        assert_eq!(lockfile.version, 2);
        assert_eq!(lockfile.packs.len(), 1);

        let pack = &lockfile.packs[0];
        assert_eq!(pack.name, "eu-ai-act-pro");
        assert_eq!(pack.version, "1.2.0");
        assert_eq!(pack.digest, "sha256:abc123def456");
        assert_eq!(pack.source, LockSource::Registry);
        assert!(pack.signature.is_some());
    }

    #[test]
    fn test_lockfile_parse_unsupported_version() {
        let yaml = r#"
version: 99
generated_at: "2026-01-29T10:00:00Z"
generated_by: "future-cli/9.0.0"
packs: []
"#;

        let result = Lockfile::parse(yaml);
        assert!(matches!(result, Err(RegistryError::Lockfile { .. })));
    }

    #[test]
    fn test_lockfile_add_pack() {
        let mut lockfile = Lockfile::new();

        let pack1 = LockedPack {
            name: "pack-b".to_string(),
            version: "1.0.0".to_string(),
            digest: "sha256:bbb".to_string(),
            source: LockSource::Registry,
            registry_url: None,
            byos_url: None,
            signature: None,
        };

        let pack2 = LockedPack {
            name: "pack-a".to_string(),
            version: "1.0.0".to_string(),
            digest: "sha256:aaa".to_string(),
            source: LockSource::Registry,
            registry_url: None,
            byos_url: None,
            signature: None,
        };

        lockfile.add_pack(pack1);
        lockfile.add_pack(pack2);

        // Should be sorted by name
        assert_eq!(lockfile.packs[0].name, "pack-a");
        assert_eq!(lockfile.packs[1].name, "pack-b");
    }

    #[test]
    fn test_lockfile_add_pack_update() {
        let mut lockfile = Lockfile::new();

        let pack1 = LockedPack {
            name: "my-pack".to_string(),
            version: "1.0.0".to_string(),
            digest: "sha256:old".to_string(),
            source: LockSource::Registry,
            registry_url: None,
            byos_url: None,
            signature: None,
        };

        let pack2 = LockedPack {
            name: "my-pack".to_string(),
            version: "1.1.0".to_string(),
            digest: "sha256:new".to_string(),
            source: LockSource::Registry,
            registry_url: None,
            byos_url: None,
            signature: None,
        };

        lockfile.add_pack(pack1);
        lockfile.add_pack(pack2);

        // Should only have one entry (updated)
        assert_eq!(lockfile.packs.len(), 1);
        assert_eq!(lockfile.packs[0].version, "1.1.0");
        assert_eq!(lockfile.packs[0].digest, "sha256:new");
    }

    #[test]
    fn test_lockfile_remove_pack() {
        let mut lockfile = Lockfile::new();

        let pack = LockedPack {
            name: "my-pack".to_string(),
            version: "1.0.0".to_string(),
            digest: "sha256:abc".to_string(),
            source: LockSource::Registry,
            registry_url: None,
            byos_url: None,
            signature: None,
        };

        lockfile.add_pack(pack);
        assert!(lockfile.contains("my-pack"));

        let removed = lockfile.remove_pack("my-pack");
        assert!(removed);
        assert!(!lockfile.contains("my-pack"));

        let removed_again = lockfile.remove_pack("my-pack");
        assert!(!removed_again);
    }

    #[test]
    fn test_lockfile_get_pack() {
        let mut lockfile = Lockfile::new();

        let pack = LockedPack {
            name: "my-pack".to_string(),
            version: "1.0.0".to_string(),
            digest: "sha256:abc".to_string(),
            source: LockSource::Registry,
            registry_url: None,
            byos_url: None,
            signature: None,
        };

        lockfile.add_pack(pack);

        let found = lockfile.get_pack("my-pack");
        assert!(found.is_some());
        assert_eq!(found.unwrap().version, "1.0.0");

        let not_found = lockfile.get_pack("other-pack");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_lockfile_to_yaml() {
        let mut lockfile = Lockfile::new();

        let pack = LockedPack {
            name: "my-pack".to_string(),
            version: "1.0.0".to_string(),
            digest: "sha256:abc123".to_string(),
            source: LockSource::Registry,
            registry_url: Some("https://registry.example.com/v1".to_string()),
            byos_url: None,
            signature: Some(LockSignature {
                algorithm: "Ed25519".to_string(),
                key_id: "sha256:key123".to_string(),
            }),
        };

        lockfile.add_pack(pack);

        let yaml = lockfile.to_yaml().unwrap();
        assert!(yaml.contains("version: 2"));
        assert!(yaml.contains("my-pack"));
        assert!(yaml.contains("sha256:abc123"));
        assert!(yaml.contains("Ed25519"));
    }

    #[test]
    fn test_lock_source_serialize() {
        let sources = vec![
            (LockSource::Bundled, "bundled"),
            (LockSource::Registry, "registry"),
            (LockSource::Byos, "byos"),
            (LockSource::Local, "local"),
        ];

        for (source, expected) in sources {
            let yaml = serde_yaml::to_string(&source).unwrap();
            assert!(yaml.contains(expected));
        }
    }
}
