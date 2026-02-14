//! Wave4 Step2 lockfile split scaffold.
//!
//! Commit A contract:
//! - `lockfile.rs` remains the active facade.
//! - Step2 routes implementation here with stable facade signatures.
//! - No behavior/perf changes intended in Step2.

pub(crate) mod digest;
pub(crate) mod errors;
pub(crate) mod format;
pub(crate) mod io;
pub(crate) mod parse;
pub(crate) mod tests;
pub(crate) mod types;

use tracing::{debug, warn};

use crate::error::RegistryResult;
use crate::reference::PackRef;
use crate::resolver::{PackResolver, ResolveSource};

use super::{LockSignature, LockSource, LockedPack, Lockfile};

pub(crate) async fn generate_lockfile_impl(
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
