mod bundled;
mod byos;
mod local;
mod registry;

#[cfg(test)]
mod tests;

use crate::cache::PackCache;
use crate::client::RegistryClient;
use crate::error::RegistryResult;
use crate::reference::PackRef;
use crate::trust::TrustStore;
use crate::types::RegistryConfig;
use crate::verify::VerifyResult;

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
        let trust_store = TrustStore::from_production_roots()?;

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
                    tracing::info!(name, version, "prefetched pack");
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
