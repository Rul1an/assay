//! Digest and mismatch-check boundary scaffold for lockfile split.
//!
//! Planned ownership (Step2+):
//! - digest normalization and compare helpers
//! - mismatch detection helpers

use chrono::Utc;
use tracing::{debug, info, warn};

use crate::error::{RegistryError, RegistryResult};
use crate::resolver::PackResolver;

use super::super::{LockMismatch, LockSource, Lockfile, VerifyLockResult};

pub(crate) async fn verify_lockfile_impl(
    lockfile: &Lockfile,
    resolver: &PackResolver,
) -> RegistryResult<VerifyLockResult> {
    let mut matched = Vec::new();
    let mut mismatched = Vec::new();
    let mut missing = Vec::new();

    for locked in &lockfile.packs {
        debug!(name = %locked.name, version = %locked.version, "verifying locked pack");

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
        extra: Vec::new(),
    })
}

pub(crate) async fn check_lockfile_impl(
    lockfile: &Lockfile,
    resolver: &PackResolver,
) -> RegistryResult<Vec<LockMismatch>> {
    let result = verify_lockfile_impl(lockfile, resolver).await?;

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

pub(crate) async fn update_lockfile_impl(
    lockfile: &mut Lockfile,
    resolver: &PackResolver,
) -> RegistryResult<Vec<String>> {
    let mut updated = Vec::new();

    for locked in &mut lockfile.packs {
        if locked.source != LockSource::Registry {
            continue;
        }

        debug!(name = %locked.name, version = %locked.version, "checking for updates");
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
