use super::KillRequest;
use std::path::PathBuf;

pub fn write_incident_bundle_pre_kill(_req: &KillRequest) -> anyhow::Result<Option<PathBuf>> {
    // Placeholder: minimal impl or no-op for now as requested
    Ok(None)
}

pub fn write_incident_bundle_post_kill(_dir: &PathBuf, _req: &KillRequest, _success: bool, _children: &[u32]) -> anyhow::Result<()> {
    Ok(())
}
