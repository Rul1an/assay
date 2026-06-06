use tracing::{debug, info};

use super::{PackResolver, ResolveSource, ResolvedPack};
use crate::error::{RegistryError, RegistryResult};
use crate::verify::compute_digest;

impl PackResolver {
    /// Resolve a BYOS URL.
    pub(super) async fn resolve_byos(&self, url: &str) -> RegistryResult<ResolvedPack> {
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
}
