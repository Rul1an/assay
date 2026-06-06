use tracing::{debug, info, warn};

use super::{PackResolver, ResolveSource, ResolvedPack};
use crate::error::{RegistryError, RegistryResult};
use crate::verify::{verify_pack, VerifyOptions};

impl PackResolver {
    /// Resolve a registry pack.
    pub(super) async fn resolve_registry(
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
}
