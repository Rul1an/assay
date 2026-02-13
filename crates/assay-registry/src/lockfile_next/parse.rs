//! Parsing-only boundary scaffold for lockfile split.
//!
//! Planned ownership (Step2+):
//! - deserialize and version-shape validation
//! - parse helpers used by lockfile facade

use crate::error::{RegistryError, RegistryResult};

use super::super::{Lockfile, LOCKFILE_VERSION};

pub(crate) fn parse_lockfile_impl(content: &str) -> RegistryResult<Lockfile> {
    let lockfile: Lockfile =
        serde_yaml::from_str(content).map_err(|e| RegistryError::Lockfile {
            message: format!("failed to parse lockfile: {}", e),
        })?;

    if lockfile.version > LOCKFILE_VERSION {
        return Err(RegistryError::Lockfile {
            message: format!(
                "lockfile version {} is newer than supported version {}",
                lockfile.version, LOCKFILE_VERSION
            ),
        });
    }

    Ok(lockfile)
}
