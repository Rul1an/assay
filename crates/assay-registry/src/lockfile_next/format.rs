//! Formatting and stable-ordering boundary scaffold for lockfile split.
//!
//! Planned ownership (Step2+):
//! - canonical output ordering
//! - serialization helpers

use chrono::Utc;

use crate::error::{RegistryError, RegistryResult};

use super::super::{LockedPack, Lockfile};

pub(crate) fn to_yaml_impl(lockfile: &Lockfile) -> RegistryResult<String> {
    serde_yaml::to_string(lockfile).map_err(|e| RegistryError::Lockfile {
        message: format!("failed to serialize lockfile: {}", e),
    })
}

pub(crate) fn add_pack_impl(lockfile: &mut Lockfile, pack: LockedPack) {
    lockfile.packs.retain(|p| p.name != pack.name);
    lockfile.packs.push(pack);
    lockfile.packs.sort_by(|a, b| a.name.cmp(&b.name));
    lockfile.generated_at = Utc::now();
}
