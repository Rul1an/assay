use crate::{CapabilitySurface, CorrelationReport, ObservationHealth};
use flate2::write::GzEncoder;
use flate2::{Compression, GzBuilder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;
use tar::{Builder, Header};
use thiserror::Error;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerSpikeArchive {
    pub run_id: String,
    pub events_ndjson: Vec<u8>,
    pub kernel_layer_ndjson: Vec<u8>,
    pub policy_layer_ndjson: Vec<u8>,
    pub sdk_layer_ndjson: Vec<u8>,
    pub capability_surface: CapabilitySurface,
    pub observation_health: ObservationHealth,
    pub correlation_report: CorrelationReport,
}

#[derive(Debug, Error)]
pub enum RunnerSpikeArchiveError {
    #[error("run_id must not be empty")]
    EmptyRunId,
    #[error("{field} run_id mismatch: expected {expected}, found {actual}")]
    RunIdMismatch {
        field: &'static str,
        expected: String,
        actual: String,
    },
    #[error("invalid observation health: {0}")]
    ObservationHealth(#[from] crate::health::ObservationHealthError),
    #[error("invalid capability surface: {0}")]
    CapabilitySurface(#[from] crate::surface::CapabilitySurfaceError),
    #[error("invalid correlation report: {0}")]
    CorrelationReport(#[from] crate::correlation::CorrelationReportError),
    #[error("json serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("archive io failed: {0}")]
    Io(#[from] std::io::Error),
}

impl RunnerSpikeArchive {
    pub fn empty(run_id: impl Into<String>, platform: impl Into<String>) -> Self {
        let run_id = run_id.into();
        Self {
            run_id: run_id.clone(),
            events_ndjson: Vec::new(),
            kernel_layer_ndjson: Vec::new(),
            policy_layer_ndjson: Vec::new(),
            sdk_layer_ndjson: Vec::new(),
            capability_surface: CapabilitySurface::new(run_id.clone()),
            observation_health: ObservationHealth::new(run_id.clone(), platform),
            correlation_report: CorrelationReport::clean(run_id),
        }
    }

    pub fn write<W: Write>(&self, writer: W) -> Result<(), RunnerSpikeArchiveError> {
        self.validate()?;

        let files = self.archive_files()?;
        let manifest = build_manifest(&self.run_id, &files);
        let manifest_bytes = serde_json::to_vec(&manifest)?;

        let mut tar = create_deterministic_tar(writer);
        write_entry(&mut tar, MANIFEST_PATH, &manifest_bytes)?;
        for (path, bytes) in files {
            write_entry(&mut tar, path, bytes.as_slice())?;
        }
        let encoder = tar.into_inner()?;
        encoder.finish()?;
        Ok(())
    }

    pub fn manifest(&self) -> Result<ArchiveManifest, RunnerSpikeArchiveError> {
        self.validate()?;
        let files = self.archive_files()?;
        Ok(build_manifest(&self.run_id, &files))
    }

    fn validate(&self) -> Result<(), RunnerSpikeArchiveError> {
        if self.run_id.is_empty() {
            return Err(RunnerSpikeArchiveError::EmptyRunId);
        }
        ensure_run_id(
            "capability_surface",
            &self.run_id,
            &self.capability_surface.run_id,
        )?;
        ensure_run_id(
            "observation_health",
            &self.run_id,
            &self.observation_health.run_id,
        )?;
        ensure_run_id(
            "correlation_report",
            &self.run_id,
            &self.correlation_report.run_id,
        )?;
        self.observation_health.validate()?;
        self.capability_surface.validate()?;
        self.correlation_report.validate()?;
        Ok(())
    }

    fn archive_files(
        &self,
    ) -> Result<BTreeMap<&'static str, ArchiveFileContent<'_>>, RunnerSpikeArchiveError> {
        let mut files = BTreeMap::new();
        files.insert(
            EVENTS_PATH,
            ArchiveFileContent::Borrowed(&self.events_ndjson),
        );
        files.insert(
            KERNEL_LAYER_PATH,
            ArchiveFileContent::Borrowed(&self.kernel_layer_ndjson),
        );
        files.insert(
            POLICY_LAYER_PATH,
            ArchiveFileContent::Borrowed(&self.policy_layer_ndjson),
        );
        files.insert(
            SDK_LAYER_PATH,
            ArchiveFileContent::Borrowed(&self.sdk_layer_ndjson),
        );
        files.insert(
            CAPABILITY_SURFACE_PATH,
            ArchiveFileContent::Owned(serde_json::to_vec(&self.capability_surface)?),
        );
        files.insert(
            OBSERVATION_HEALTH_PATH,
            ArchiveFileContent::Owned(serde_json::to_vec(&self.observation_health)?),
        );
        files.insert(
            CORRELATION_REPORT_PATH,
            ArchiveFileContent::Owned(serde_json::to_vec(&self.correlation_report)?),
        );
        Ok(files)
    }
}

enum ArchiveFileContent<'a> {
    Borrowed(&'a [u8]),
    Owned(Vec<u8>),
}

impl ArchiveFileContent<'_> {
    fn as_slice(&self) -> &[u8] {
        match self {
            Self::Borrowed(bytes) => bytes,
            Self::Owned(bytes) => bytes,
        }
    }
}

fn ensure_run_id(
    field: &'static str,
    expected: &str,
    actual: &str,
) -> Result<(), RunnerSpikeArchiveError> {
    if expected == actual {
        return Ok(());
    }
    Err(RunnerSpikeArchiveError::RunIdMismatch {
        field,
        expected: expected.to_string(),
        actual: actual.to_string(),
    })
}

fn build_manifest(
    run_id: &str,
    files: &BTreeMap<&'static str, ArchiveFileContent<'_>>,
) -> ArchiveManifest {
    let files = files
        .iter()
        .map(|(path, content)| {
            let bytes = content.as_slice();
            (
                (*path).to_string(),
                ArchiveFile {
                    path: (*path).to_string(),
                    sha256: sha256_prefixed(bytes),
                    bytes: bytes.len() as u64,
                },
            )
        })
        .collect();

    ArchiveManifest {
        schema: ARCHIVE_MANIFEST_SCHEMA.to_string(),
        run_id: run_id.to_string(),
        files,
    }
}

fn sha256_prefixed(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

fn create_deterministic_tar<W: Write>(writer: W) -> Builder<GzEncoder<W>> {
    let encoder = GzBuilder::new()
        .mtime(0)
        .operating_system(255)
        .write(writer, Compression::best());

    let mut tar = Builder::new(encoder);
    tar.mode(tar::HeaderMode::Deterministic);
    tar
}

fn write_entry<T: Write>(tar: &mut Builder<T>, path: &str, data: &[u8]) -> std::io::Result<()> {
    let mut header = Header::new_gnu();
    header.set_path(path)?;
    header.set_size(data.len() as u64);
    header.set_mode(0o644);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_username("assay")?;
    header.set_groupname("assay")?;
    header.set_cksum();

    tar.append(&header, data)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::GzDecoder;
    use std::io::{Cursor, Read};

    #[test]
    fn empty_archive_preserves_required_files_and_layers() {
        let archive = RunnerSpikeArchive::empty("run_001", "linux");
        let (order, entries) = archive_entries(&archive);

        assert_eq!(order.first().map(String::as_str), Some(MANIFEST_PATH));
        for path in [
            MANIFEST_PATH,
            CAPABILITY_SURFACE_PATH,
            CORRELATION_REPORT_PATH,
            EVENTS_PATH,
            KERNEL_LAYER_PATH,
            OBSERVATION_HEALTH_PATH,
            POLICY_LAYER_PATH,
            SDK_LAYER_PATH,
        ] {
            assert!(entries.contains_key(path), "missing archive entry {path}");
        }
        assert!(entries[EVENTS_PATH].is_empty());
        assert!(entries[KERNEL_LAYER_PATH].is_empty());
        assert!(entries[POLICY_LAYER_PATH].is_empty());
        assert!(entries[SDK_LAYER_PATH].is_empty());
    }

    #[test]
    fn manifest_hashes_match_archive_payloads() {
        let mut archive = RunnerSpikeArchive::empty("run_001", "linux");
        archive.events_ndjson = b"{\"event\":\"started\"}\n".to_vec();
        let (_, entries) = archive_entries(&archive);
        let manifest: ArchiveManifest = serde_json::from_slice(&entries[MANIFEST_PATH]).unwrap();

        for (path, file) in &manifest.files {
            let bytes = &entries[path.as_str()];
            assert_eq!(file.path, *path);
            assert_eq!(file.bytes, bytes.len() as u64);
            assert_eq!(file.sha256, sha256_prefixed(bytes));
        }
    }

    #[test]
    fn archive_bytes_are_deterministic() {
        let archive = RunnerSpikeArchive::empty("run_001", "linux");
        let mut first = Vec::new();
        let mut second = Vec::new();

        archive.write(&mut first).unwrap();
        archive.write(&mut second).unwrap();

        assert_eq!(first, second);
    }

    #[test]
    fn write_rejects_run_id_mismatch() {
        let mut archive = RunnerSpikeArchive::empty("run_001", "linux");
        archive.capability_surface.run_id = "run_002".to_string();
        let mut bytes = Vec::new();

        let err = archive.write(&mut bytes).unwrap_err();

        assert!(matches!(
            err,
            RunnerSpikeArchiveError::RunIdMismatch {
                field: "capability_surface",
                ..
            }
        ));
    }

    #[test]
    fn write_rejects_invalid_observation_health() {
        let mut archive = RunnerSpikeArchive::empty("run_001", "linux");
        archive.observation_health.ringbuf_drops = 1;
        archive.observation_health.kernel_layer = crate::KernelLayerStatus::Complete;
        let mut bytes = Vec::new();

        let err = archive.write(&mut bytes).unwrap_err();

        assert!(matches!(
            err,
            RunnerSpikeArchiveError::ObservationHealth(
                crate::health::ObservationHealthError::RingbufDropsRequirePartialKernelLayer
            )
        ));
    }

    #[test]
    fn write_rejects_invalid_capability_surface_schema() {
        let mut archive = RunnerSpikeArchive::empty("run_001", "linux");
        archive.capability_surface.schema = "assay.runner.capability_surface.v_future".to_string();
        let mut bytes = Vec::new();

        let err = archive.write(&mut bytes).unwrap_err();

        assert!(matches!(
            err,
            RunnerSpikeArchiveError::CapabilitySurface(
                crate::surface::CapabilitySurfaceError::InvalidSchema
            )
        ));
    }

    #[test]
    fn write_rejects_invalid_correlation_report_schema() {
        let mut archive = RunnerSpikeArchive::empty("run_001", "linux");
        archive.correlation_report.schema = "assay.runner.correlation_report.v_future".to_string();
        let mut bytes = Vec::new();

        let err = archive.write(&mut bytes).unwrap_err();

        assert!(matches!(
            err,
            RunnerSpikeArchiveError::CorrelationReport(
                crate::correlation::CorrelationReportError::InvalidSchema
            )
        ));
    }

    fn archive_entries(archive: &RunnerSpikeArchive) -> (Vec<String>, BTreeMap<String, Vec<u8>>) {
        let mut bytes = Vec::new();
        archive.write(&mut bytes).unwrap();

        let decoder = GzDecoder::new(Cursor::new(bytes));
        let mut tar = tar::Archive::new(decoder);
        let mut order = Vec::new();
        let mut entries = BTreeMap::new();
        for entry in tar.entries().unwrap() {
            let mut entry = entry.unwrap();
            let path = entry.path().unwrap().to_string_lossy().into_owned();
            order.push(path.clone());
            let mut data = Vec::new();
            entry.read_to_end(&mut data).unwrap();
            entries.insert(path, data);
        }
        (order, entries)
    }
}
