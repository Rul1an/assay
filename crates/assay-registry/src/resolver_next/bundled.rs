use std::path::Path;

use tokio::fs;
use tracing::{debug, info};

use super::{PackResolver, ResolveSource, ResolvedPack};
use crate::error::{RegistryError, RegistryResult};
use crate::verify::compute_digest;

impl PackResolver {
    /// Resolve a bundled pack.
    pub(super) async fn resolve_bundled(&self, name: &str) -> RegistryResult<ResolvedPack> {
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
}
