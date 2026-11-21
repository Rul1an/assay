//! Parsing-only boundary scaffold for lockfile split.
//!
//! Planned ownership (Step2+):
//! - deserialize and version-shape validation
//! - parse helpers used by lockfile facade

use crate::error::{RegistryError, RegistryResult};

use super::super::{Lockfile, LOCKFILE_VERSION};

pub(crate) fn parse_lockfile_impl(content: &str) -> RegistryResult<Lockfile> {
    use assay_common::doc::Refusal;

    super::check_size(content.len())?;
    let lockfile: Lockfile = assay_common::doc::load(content).map_err(|error| {
        let reason = match error {
            Refusal::UnknownKeys(paths) => format!("unknown lockfile keys: {paths:?}"),
            Refusal::NotAMapping { path, expected } => {
                format!("expected a {expected} mapping at {path:?}")
            }
            Refusal::Syntax { message, .. } | Refusal::Shape { message, .. } => message,
            _ => "invalid lockfile".to_owned(),
        };
        RegistryError::Lockfile {
            message: format!("failed to parse lockfile: {reason}"),
        }
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
