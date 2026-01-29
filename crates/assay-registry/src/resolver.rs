//! Pack resolution.
//!
//! Resolves pack references to content with the following priority:
//! 1. Local file (if path exists)
//! 2. Bundled pack (compiled into binary)
//! 3. Cache (if valid and not expired)
//! 4. Registry (remote fetch)
//! 5. BYOS (Bring Your Own Storage)

use std::path::Path;

use tokio::fs;
use tracing::{debug, info, warn};

use crate::cache::PackCache;
use crate::client::RegistryClient;
use crate::error::{RegistryError, RegistryResult};
use crate::reference::PackRef;
use crate::trust::TrustStore;
use crate::types::RegistryConfig;
use crate::verify::{compute_digest, verify_pack, VerifyOptions, VerifyResult};

/// Resolved pack content.
#[derive(Debug, Clone)]
pub struct ResolvedPack {
    /// Pack YAML content.
    pub content: String,

    /// Where the pack was resolved from.
    pub source: ResolveSource,

    /// Content digest.
    pub digest: String,

    /// Verification result (if verified).
    pub verification: Option<VerifyResult>,
}

/// Source of a resolved pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveSource {
    /// Local file.
    Local(String),

    /// Bundled with the binary.
    Bundled(String),

    /// From local cache.
    Cache,

    /// Fetched from registry.
    Registry(String),

    /// Fetched from BYOS.
    Byos(String),
}

impl std::fmt::Display for ResolveSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local(path) => write!(f, "local:{}", path),
            Self::Bundled(name) => write!(f, "bundled:{}", name),
            Self::Cache => write!(f, "cache"),
            Self::Registry(url) => write!(f, "registry:{}", url),
            Self::Byos(url) => write!(f, "byos:{}", url),
        }
    }
}

/// Pack resolver configuration.
#[derive(Debug, Clone)]
pub struct ResolverConfig {
    /// Registry configuration.
    pub registry: RegistryConfig,

    /// Skip cache lookup.
    pub no_cache: bool,

    /// Allow unsigned packs.
    pub allow_unsigned: bool,

    /// Directory containing bundled packs.
    pub bundled_packs_dir: Option<String>,
}

impl Default for ResolverConfig {
    fn default() -> Self {
        Self {
            registry: RegistryConfig::from_env(),
            no_cache: false,
            allow_unsigned: false,
            bundled_packs_dir: None,
        }
    }
}

impl ResolverConfig {
    /// Skip cache.
    pub fn no_cache(mut self) -> Self {
        self.no_cache = true;
        self
    }

    /// Allow unsigned packs.
    pub fn allow_unsigned(mut self) -> Self {
        self.allow_unsigned = true;
        self
    }

    /// Set bundled packs directory.
    pub fn with_bundled_dir(mut self, dir: impl Into<String>) -> Self {
        self.bundled_packs_dir = Some(dir.into());
        self
    }
}

/// Pack resolver.
pub struct PackResolver {
    /// Registry client.
    client: RegistryClient,

    /// Local cache.
    cache: PackCache,

    /// Trust store for signature verification.
    trust_store: TrustStore,

    /// Configuration.
    config: ResolverConfig,
}

impl PackResolver {
    /// Create a new resolver with default configuration.
    pub fn new() -> RegistryResult<Self> {
        Self::with_config(ResolverConfig::default())
    }

    /// Create a resolver with custom configuration.
    pub fn with_config(config: ResolverConfig) -> RegistryResult<Self> {
        let client = RegistryClient::new(config.registry.clone())?;
        let cache = PackCache::new()?;
        let trust_store = TrustStore::new();

        Ok(Self {
            client,
            cache,
            trust_store,
            config,
        })
    }

    /// Create a resolver for testing with custom components.
    pub fn with_components(
        client: RegistryClient,
        cache: PackCache,
        trust_store: TrustStore,
        config: ResolverConfig,
    ) -> Self {
        Self {
            client,
            cache,
            trust_store,
            config,
        }
    }

    /// Resolve a pack reference to content.
    pub async fn resolve(&self, reference: &str) -> RegistryResult<ResolvedPack> {
        let pack_ref = PackRef::parse(reference)?;
        self.resolve_ref(&pack_ref).await
    }

    /// Resolve a parsed pack reference.
    pub async fn resolve_ref(&self, pack_ref: &PackRef) -> RegistryResult<ResolvedPack> {
        match pack_ref {
            PackRef::Local(path) => self.resolve_local(path).await,
            PackRef::Bundled(name) => self.resolve_bundled(name).await,
            PackRef::Registry {
                name,
                version,
                pinned_digest,
            } => {
                self.resolve_registry(name, version, pinned_digest.as_deref())
                    .await
            }
            PackRef::Byos(url) => self.resolve_byos(url).await,
        }
    }

    /// Resolve a local file.
    async fn resolve_local(&self, path: &Path) -> RegistryResult<ResolvedPack> {
        debug!(path = %path.display(), "resolving local file");

        if !path.exists() {
            return Err(RegistryError::NotFound {
                name: path.display().to_string(),
                version: "local".to_string(),
            });
        }

        let content = fs::read_to_string(path)
            .await
            .map_err(|e| RegistryError::Cache {
                message: format!("failed to read local file: {}", e),
            })?;

        let digest = compute_digest(&content);

        info!(path = %path.display(), digest = %digest, "resolved local pack");

        Ok(ResolvedPack {
            content,
            source: ResolveSource::Local(path.display().to_string()),
            digest,
            verification: None, // Local files are not verified
        })
    }

    /// Resolve a bundled pack.
    async fn resolve_bundled(&self, name: &str) -> RegistryResult<ResolvedPack> {
        debug!(name, "resolving bundled pack");

        // Check configured bundled packs directory
        if let Some(dir) = &self.config.bundled_packs_dir {
            let pack_path = Path::new(dir).join(format!("{}.yaml", name));
            if pack_path.exists() {
                let content =
                    fs::read_to_string(&pack_path)
                        .await
                        .map_err(|e| RegistryError::Cache {
                            message: format!("failed to read bundled pack: {}", e),
                        })?;

                let digest = compute_digest(&content);
                info!(name, digest = %digest, "resolved bundled pack");

                return Ok(ResolvedPack {
                    content,
                    source: ResolveSource::Bundled(name.to_string()),
                    digest,
                    verification: None,
                });
            }
        }

        // Look for bundled packs in standard locations
        let standard_paths = [
            format!("packs/open/{}.yaml", name),
            format!("packs/{}.yaml", name),
        ];

        for relative_path in &standard_paths {
            let path = Path::new(relative_path);
            if path.exists() {
                let content = fs::read_to_string(path)
                    .await
                    .map_err(|e| RegistryError::Cache {
                        message: format!("failed to read bundled pack: {}", e),
                    })?;

                let digest = compute_digest(&content);
                info!(name, path = %path.display(), digest = %digest, "resolved bundled pack");

                return Ok(ResolvedPack {
                    content,
                    source: ResolveSource::Bundled(name.to_string()),
                    digest,
                    verification: None,
                });
            }
        }

        Err(RegistryError::NotFound {
            name: name.to_string(),
            version: "bundled".to_string(),
        })
    }

    /// Resolve a registry pack.
    async fn resolve_registry(
        &self,
        name: &str,
        version: &str,
        pinned_digest: Option<&str>,
    ) -> RegistryResult<ResolvedPack> {
        debug!(name, version, pinned_digest, "resolving registry pack");

        // 1. Check cache first (unless --no-cache)
        if !self.config.no_cache {
            if let Some(cached) = self.try_cache(name, version, pinned_digest).await? {
                return Ok(cached);
            }
        }

        // 2. Fetch from registry
        let etag = if self.config.no_cache {
            None
        } else {
            self.cache.get_etag(name, version).await
        };

        let result = self
            .client
            .fetch_pack(name, version, etag.as_deref())
            .await?;

        let fetch_result =
            match result {
                Some(r) => r,
                None => {
                    // 304 Not Modified - use cached version
                    let cached_entry = self.cache.get(name, version).await?.ok_or_else(|| {
                        RegistryError::Cache {
                            message: "304 response but no cached entry".to_string(),
                        }
                    })?;

                    return Ok(ResolvedPack {
                        content: cached_entry.content,
                        source: ResolveSource::Cache,
                        digest: cached_entry.metadata.digest.clone(),
                        verification: None,
                    });
                }
            };

        // 3. Verify digest if pinned
        if let Some(expected_digest) = pinned_digest {
            if fetch_result.computed_digest != expected_digest {
                return Err(RegistryError::DigestMismatch {
                    name: name.to_string(),
                    version: version.to_string(),
                    expected: expected_digest.to_string(),
                    actual: fetch_result.computed_digest.clone(),
                });
            }
        }

        // 4. Verify signature
        let verify_options = VerifyOptions {
            allow_unsigned: self.config.allow_unsigned,
            skip_signature: false,
        };

        let verification = match verify_pack(&fetch_result, &self.trust_store, &verify_options) {
            Ok(v) => Some(v),
            Err(e) => {
                // If unsigned and allowed, continue
                if self.config.allow_unsigned {
                    warn!(name, version, error = %e, "pack verification failed, but unsigned allowed");
                    None
                } else {
                    return Err(e);
                }
            }
        };

        // 5. Cache the result
        if !self.config.no_cache {
            if let Err(e) = self
                .cache
                .put(name, version, &fetch_result, Some(self.client.base_url()))
                .await
            {
                warn!(name, version, error = %e, "failed to cache pack");
            }
        }

        let digest = fetch_result.computed_digest.clone();
        info!(name, version, digest = %digest, "resolved registry pack");

        Ok(ResolvedPack {
            content: fetch_result.content,
            source: ResolveSource::Registry(self.client.base_url().to_string()),
            digest,
            verification,
        })
    }

    /// Try to get pack from cache.
    async fn try_cache(
        &self,
        name: &str,
        version: &str,
        pinned_digest: Option<&str>,
    ) -> RegistryResult<Option<ResolvedPack>> {
        match self.cache.get(name, version).await {
            Ok(Some(entry)) => {
                // Check pinned digest if provided
                if let Some(expected) = pinned_digest {
                    if entry.metadata.digest != expected {
                        debug!(
                            name,
                            version,
                            expected,
                            actual = %entry.metadata.digest,
                            "cached digest does not match pinned, evicting"
                        );
                        self.cache.evict(name, version).await?;
                        return Ok(None);
                    }
                }

                info!(name, version, "using cached pack");
                Ok(Some(ResolvedPack {
                    content: entry.content,
                    source: ResolveSource::Cache,
                    digest: entry.metadata.digest,
                    verification: None,
                }))
            }
            Ok(None) => Ok(None),
            Err(RegistryError::DigestMismatch { .. }) => {
                // Cache corruption - evict and re-fetch
                warn!(name, version, "cache integrity check failed, evicting");
                self.cache.evict(name, version).await?;
                Ok(None)
            }
            Err(e) => {
                warn!(name, version, error = %e, "cache read error");
                Ok(None)
            }
        }
    }

    /// Resolve a BYOS URL.
    async fn resolve_byos(&self, url: &str) -> RegistryResult<ResolvedPack> {
        debug!(url, "resolving BYOS pack");

        // For now, only support HTTPS URLs directly
        if url.starts_with("https://") || url.starts_with("http://") {
            let response = reqwest::get(url)
                .await
                .map_err(|e| RegistryError::Network {
                    message: format!("failed to fetch BYOS pack: {}", e),
                })?;

            if !response.status().is_success() {
                return Err(RegistryError::NotFound {
                    name: url.to_string(),
                    version: "byos".to_string(),
                });
            }

            let content = response.text().await.map_err(|e| RegistryError::Network {
                message: format!("failed to read BYOS response: {}", e),
            })?;

            let digest = compute_digest(&content);
            info!(url, digest = %digest, "resolved BYOS pack");

            return Ok(ResolvedPack {
                content,
                source: ResolveSource::Byos(url.to_string()),
                digest,
                verification: None,
            });
        }

        // S3, GCS, Azure would require object_store integration
        // For now, return not implemented error
        Err(RegistryError::Config {
            message: format!("BYOS scheme not yet supported: {}", url),
        })
    }

    /// Pre-fetch a pack for offline use.
    pub async fn prefetch(&self, reference: &str) -> RegistryResult<()> {
        let pack_ref = PackRef::parse(reference)?;

        match &pack_ref {
            PackRef::Registry { name, version, .. } => {
                // Fetch and cache
                let result = self.client.fetch_pack(name, version, None).await?;

                if let Some(fetch_result) = result {
                    self.cache
                        .put(name, version, &fetch_result, Some(self.client.base_url()))
                        .await?;
                    info!(name, version, "prefetched pack");
                }
                Ok(())
            }
            _ => {
                // Nothing to prefetch for local/bundled
                Ok(())
            }
        }
    }

    /// Get the cache.
    pub fn cache(&self) -> &PackCache {
        &self.cache
    }

    /// Get the trust store.
    pub fn trust_store(&self) -> &TrustStore {
        &self.trust_store
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_resolve_local_file() {
        let temp_dir = TempDir::new().unwrap();
        let pack_path = temp_dir.path().join("test.yaml");
        fs::write(&pack_path, "name: test\nversion: 1.0.0")
            .await
            .unwrap();

        let config = ResolverConfig::default().allow_unsigned();
        let resolver = PackResolver::with_config(config).unwrap();

        let result = resolver.resolve(pack_path.to_str().unwrap()).await.unwrap();

        assert!(matches!(result.source, ResolveSource::Local(_)));
        assert!(result.content.contains("name: test"));
    }

    #[tokio::test]
    async fn test_resolve_local_file_not_found() {
        let config = ResolverConfig::default().allow_unsigned();
        let resolver = PackResolver::with_config(config).unwrap();

        let result = resolver.resolve("/nonexistent/pack.yaml").await;
        assert!(matches!(result, Err(RegistryError::NotFound { .. })));
    }

    #[tokio::test]
    async fn test_resolve_bundled_not_found() {
        let config = ResolverConfig::default().allow_unsigned();
        let resolver = PackResolver::with_config(config).unwrap();

        let result = resolver.resolve("nonexistent-pack").await;
        assert!(matches!(result, Err(RegistryError::NotFound { .. })));
    }

    #[tokio::test]
    async fn test_resolve_bundled_from_config_dir() {
        let temp_dir = TempDir::new().unwrap();
        let pack_path = temp_dir.path().join("my-pack.yaml");
        fs::write(&pack_path, "name: my-pack\nversion: 1.0.0")
            .await
            .unwrap();

        let config = ResolverConfig::default()
            .allow_unsigned()
            .with_bundled_dir(temp_dir.path().to_str().unwrap());
        let resolver = PackResolver::with_config(config).unwrap();

        let result = resolver.resolve("my-pack").await.unwrap();

        assert!(matches!(result.source, ResolveSource::Bundled(_)));
        assert!(result.content.contains("name: my-pack"));
    }

    #[test]
    fn test_resolve_source_display() {
        assert_eq!(
            ResolveSource::Local("/path/to/pack.yaml".to_string()).to_string(),
            "local:/path/to/pack.yaml"
        );
        assert_eq!(
            ResolveSource::Bundled("my-pack".to_string()).to_string(),
            "bundled:my-pack"
        );
        assert_eq!(ResolveSource::Cache.to_string(), "cache");
        assert_eq!(
            ResolveSource::Registry("https://registry.example.com".to_string()).to_string(),
            "registry:https://registry.example.com"
        );
        assert_eq!(
            ResolveSource::Byos("s3://bucket/pack.yaml".to_string()).to_string(),
            "byos:s3://bucket/pack.yaml"
        );
    }
}
