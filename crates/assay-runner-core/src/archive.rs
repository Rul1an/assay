use crate::redact::{RedactionTally, Redactor};
use assay_runner_schema::{
    ArchiveFile, ArchiveManifest, CapabilitySurface, CapabilitySurfaceError, CorrelationReport,
    CorrelationReportError, ObservationHealth, ObservationHealthError, ARCHIVE_MANIFEST_SCHEMA,
    CAPABILITY_SURFACE_PATH, CORRELATION_REPORT_PATH, EVENTS_PATH, KERNEL_LAYER_PATH,
    MANIFEST_PATH, OBSERVATION_HEALTH_PATH, POLICY_LAYER_PATH, SDK_LAYER_PATH,
};
use flate2::write::GzEncoder;
use flate2::{Compression, GzBuilder};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use tar::{Builder, Header};
use thiserror::Error;

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
    ObservationHealth(#[from] ObservationHealthError),
    #[error("invalid capability surface: {0}")]
    CapabilitySurface(#[from] CapabilitySurfaceError),
    #[error("invalid correlation report: {0}")]
    CorrelationReport(#[from] CorrelationReportError),
    #[error("json serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("archive io failed: {0}")]
    Io(#[from] std::io::Error),
    #[error(
        "fail-closed redaction sweep found an unredacted {rule}-shaped value in {file}; \
         a capture funnel was missed (runner bug)"
    )]
    UnredactedSecret {
        rule: &'static str,
        file: &'static str,
    },
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

    /// Redact secret-shaped values across the in-memory archive (ADR-034), before any hashing or
    /// serialization to disk. Rewrites the capability surface string sets and the structured JSON of
    /// the event/kernel/policy/sdk ndjson streams (a clean line is left byte-identical; only a line
    /// carrying a secret is reparsed and re-emitted). Returns the value-free tally for the
    /// `observation_health.redaction` block. The raw value never reaches a hashed or stored artifact.
    pub fn redact_in_place(&mut self, redactor: &Redactor) -> RedactionTally {
        let mut tally = RedactionTally::default();
        redact_set(
            &mut self.capability_surface.filesystem_paths,
            "filesystem_paths",
            redactor,
            &mut tally,
        );
        redact_set(
            &mut self.capability_surface.network_endpoints,
            "network_endpoints",
            redactor,
            &mut tally,
        );
        redact_command_set(
            &mut self.capability_surface.process_execs,
            "process_execs",
            redactor,
            &mut tally,
        );
        redact_set(
            &mut self.capability_surface.mcp_tools,
            "mcp_tools",
            redactor,
            &mut tally,
        );
        redact_set(
            &mut self.capability_surface.policy_decisions,
            "policy_decisions",
            redactor,
            &mut tally,
        );
        self.events_ndjson = redact_ndjson(&self.events_ndjson, redactor, &mut tally);
        self.kernel_layer_ndjson = redact_ndjson(&self.kernel_layer_ndjson, redactor, &mut tally);
        self.policy_layer_ndjson = redact_ndjson(&self.policy_layer_ndjson, redactor, &mut tally);
        self.sdk_layer_ndjson = redact_ndjson(&self.sdk_layer_ndjson, redactor, &mut tally);
        tally
    }

    /// Fail-closed assertion sweep (ADR-034): after redaction, no archive file may still carry a
    /// secret-shaped value. This never rewrites bytes; it asserts. A hit means a capture funnel was
    /// missed, so bundle creation must fail rather than ship a raw secret. Runs before hashing.
    pub fn assert_no_unredacted(&self, redactor: &Redactor) -> Result<(), RunnerSpikeArchiveError> {
        let files = self.archive_files()?;
        for (path, content) in &files {
            let text = String::from_utf8_lossy(content.as_slice());
            if let Some(rule) = redactor.find_unredacted(&text) {
                return Err(RunnerSpikeArchiveError::UnredactedSecret { rule, file: path });
            }
        }
        Ok(())
    }
}

fn redact_set(
    set: &mut BTreeSet<String>,
    field: &str,
    redactor: &Redactor,
    tally: &mut RedactionTally,
) {
    let redacted: BTreeSet<String> = set
        .iter()
        .map(|v| redactor.redact_value(field, v, tally).into_owned())
        .collect();
    *set = redacted;
}

/// A process-exec value can be a full invocation (`python script.py --token X`), so it gets the
/// flag-aware argv treatment, not just a shape pass. Tokenized on whitespace; a value that changed is
/// rejoined with single spaces (process-exec strings are heuristic capture, exact spacing is not
/// load-bearing), a clean value is left exactly as is.
fn redact_command_set(
    set: &mut BTreeSet<String>,
    field: &str,
    redactor: &Redactor,
    tally: &mut RedactionTally,
) {
    let redacted: BTreeSet<String> = set
        .iter()
        .map(|v| {
            let argv: Vec<String> = v.split_whitespace().map(str::to_string).collect();
            if argv.is_empty() {
                return v.clone();
            }
            let before = tally.total;
            let out = redactor.redact_argv(field, &argv, tally);
            if tally.total > before {
                out.join(" ")
            } else {
                v.clone()
            }
        })
        .collect();
    *set = redacted;
}

fn redact_ndjson(bytes: &[u8], redactor: &Redactor, tally: &mut RedactionTally) -> Vec<u8> {
    if bytes.is_empty() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(bytes);
    let mut out = String::with_capacity(text.len());
    for segment in text.split_inclusive('\n') {
        let (content, newline) = match segment.strip_suffix('\n') {
            Some(c) => (c, "\n"),
            None => (segment, ""),
        };
        if content.trim().is_empty() {
            out.push_str(segment);
            continue;
        }
        match serde_json::from_str::<Value>(content) {
            Ok(mut value) => {
                let before = tally.total;
                redact_json_value("", &mut value, redactor, tally);
                if tally.total > before {
                    // Only a line that actually carried a secret is re-emitted.
                    match serde_json::to_string(&value) {
                        Ok(s) => {
                            out.push_str(&s);
                            out.push_str(newline);
                        }
                        Err(_) => out.push_str(segment),
                    }
                } else {
                    out.push_str(segment);
                }
            }
            // Non-JSON line (should not happen for these streams): leave untouched. The assertion
            // sweep is the backstop if a raw secret somehow survives here.
            Err(_) => out.push_str(segment),
        }
    }
    out.into_bytes()
}

fn redact_json_value(
    field: &str,
    value: &mut Value,
    redactor: &Redactor,
    tally: &mut RedactionTally,
) {
    match value {
        Value::String(s) => {
            let owned = match redactor.redact_value(field, s, tally) {
                std::borrow::Cow::Borrowed(_) => None,
                std::borrow::Cow::Owned(o) => Some(o),
            };
            if let Some(o) = owned {
                *s = o;
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                redact_json_value(field, item, redactor, tally);
            }
        }
        Value::Object(map) => {
            for (key, val) in map.iter_mut() {
                // An argv array ("command") gets the flag-aware treatment, so a value following a
                // credential flag is redacted even when it is not shape-matchable.
                if key == "command" {
                    if let Value::Array(items) = val {
                        if !items.is_empty() && items.iter().all(Value::is_string) {
                            let argv: Vec<String> = items
                                .iter()
                                .map(|i| i.as_str().unwrap_or_default().to_string())
                                .collect();
                            let redacted = redactor.redact_argv("command", &argv, tally);
                            *val = Value::Array(redacted.into_iter().map(Value::String).collect());
                            continue;
                        }
                    }
                }
                redact_json_value(key, val, redactor, tally);
            }
        }
        _ => {}
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
        archive.observation_health.kernel_layer = assay_runner_schema::KernelLayerStatus::Complete;
        let mut bytes = Vec::new();

        let err = archive.write(&mut bytes).unwrap_err();

        assert!(matches!(
            err,
            RunnerSpikeArchiveError::ObservationHealth(
                assay_runner_schema::ObservationHealthError::RingbufDropsRequirePartialKernelLayer
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
                assay_runner_schema::CapabilitySurfaceError::InvalidSchema
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
                assay_runner_schema::CorrelationReportError::InvalidSchema
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
