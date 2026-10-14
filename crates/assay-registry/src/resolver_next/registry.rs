use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use tracing::{debug, info, warn};

use super::{PackResolver, ResolveSource, ResolvedPack};
use crate::error::{RegistryError, RegistryResult};
use crate::types::{FetchResult, PackHeaders};
use crate::verify::{verify_pack, VerifyOptions, VerifyResult};

impl PackResolver {
    /// Resolve a registry pack.
    pub(super) async fn resolve_registry(
        &self,
        name: &str,
        version: &str,
        pinned_digest: Option<&str>,
    ) -> RegistryResult<ResolvedPack> {
        debug!(name, version, pinned_digest, "resolving registry pack");

        // Check cache first (unless --no-cache)
        if !self.config.no_cache {
            if let Some(cached) = self.try_cache(name, version, pinned_digest).await? {
                return Ok(cached);
            }
        }

        let (fetch_result, verification) = self
            .fetch_verified_registry(name, version, pinned_digest)
            .await?;

        // Cache the verified result
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
            verification: Some(verification),
        })
    }

    /// Fetch full content: a 304 cannot establish the current resolver policy.
    pub(super) async fn fetch_verified_registry(
        &self,
        name: &str,
        version: &str,
        pinned_digest: Option<&str>,
    ) -> RegistryResult<(FetchResult, VerifyResult)> {
        let fetched = self
            .client
            .fetch_pack(name, version, None)
            .await?
            .ok_or_else(|| RegistryError::InvalidResponse {
                message: "unexpected 304 response to unconditional pack request".to_string(),
            })?;
        let verification = self.verify_registry_pack(name, version, pinned_digest, &fetched)?;
        Ok((fetched, verification))
    }

    /// One admission rule for network, prefetch and cache content.
    fn verify_registry_pack(
        &self,
        name: &str,
        version: &str,
        pinned_digest: Option<&str>,
        fetched: &FetchResult,
    ) -> RegistryResult<VerifyResult> {
        if let Some(expected) = pinned_digest {
            if fetched.computed_digest != expected {
                return Err(RegistryError::DigestMismatch {
                    name: name.to_string(),
                    version: version.to_string(),
                    expected: expected.to_string(),
                    actual: fetched.computed_digest.clone(),
                });
            }
        }
        verify_pack(
            fetched,
            &self.trust_store,
            &VerifyOptions {
                allow_unsigned: self.config.allow_unsigned,
                skip_signature: false,
            },
        )
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
                if entry.metadata.registry_url.as_deref() != Some(self.client.base_url()) {
                    debug!(name, version, "cached registry does not match, refetching");
                    return Ok(None);
                }
                // Adapt the stored envelope to the existing verifier; never treat
                // a self-consistent cache digest as signature authority.
                let signature = entry
                    .signature
                    .as_ref()
                    .map(serde_json::to_vec)
                    .transpose()
                    .map_err(|e| RegistryError::Cache {
                        message: format!("failed to encode cached signature: {e}"),
                    })?
                    .map(|bytes| BASE64.encode(bytes));
                let fetched = FetchResult {
                    content: entry.content,
                    computed_digest: entry.metadata.digest.clone(),
                    headers: PackHeaders {
                        digest: Some(entry.metadata.digest),
                        signature,
                        key_id: entry.metadata.key_id,
                        etag: None,
                        cache_control: None,
                        content_length: None,
                    },
                };
                let verification =
                    match self.verify_registry_pack(name, version, pinned_digest, &fetched) {
                        Ok(verified) => verified,
                        Err(RegistryError::DigestMismatch { .. }) => {
                            self.cache.evict(name, version).await?;
                            return Ok(None);
                        }
                        Err(error) => return Err(error),
                    };
                info!(name, version, "using verified cached pack");
                Ok(Some(ResolvedPack {
                    content: fetched.content,
                    source: ResolveSource::Cache,
                    digest: fetched.computed_digest,
                    verification: Some(verification),
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
