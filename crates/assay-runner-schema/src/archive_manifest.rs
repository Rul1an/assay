use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const ARCHIVE_MANIFEST_SCHEMA: &str = "assay.runner.archive_manifest.v0";

pub const MANIFEST_PATH: &str = "manifest.json";
pub const EVENTS_PATH: &str = "events.ndjson";
pub const KERNEL_LAYER_PATH: &str = "layers/kernel.ndjson";
pub const POLICY_LAYER_PATH: &str = "layers/policy.ndjson";
pub const SDK_LAYER_PATH: &str = "layers/sdk.ndjson";
pub const CAPABILITY_SURFACE_PATH: &str = "capability-surface.json";
pub const OBSERVATION_HEALTH_PATH: &str = "observation-health.json";
pub const CORRELATION_REPORT_PATH: &str = "correlation-report.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveFile {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveManifest {
    pub schema: String,
    pub run_id: String,
    pub files: BTreeMap<String, ArchiveFile>,
}
