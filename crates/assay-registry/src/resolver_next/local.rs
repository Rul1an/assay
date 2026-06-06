use std::path::Path;

use tokio::fs;
use tracing::{debug, info};

use super::{PackResolver, ResolveSource, ResolvedPack};
use crate::error::{RegistryError, RegistryResult};
use crate::verify::compute_digest;

impl PackResolver {
    /// Resolve a local file.
    pub(super) async fn resolve_local(&self, path: &Path) -> RegistryResult<ResolvedPack> {
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
}
