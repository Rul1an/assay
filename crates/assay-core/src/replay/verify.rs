//! Replay bundle verification (E9b).
//!
//! Validates bundle integrity (hashes) and runs secret scan: hard fail for
//! cassettes/ and files/, warn for outputs/. See E9-REPLAY-BUNDLE-PLAN §2.5.

use crate::replay::bundle::limits::classify_source_ceiling;
use crate::replay::bundle::{paths, read_bundle_tar_gz_with_limits, ReadBundle, ReplayLimits};
use crate::replay::scrub::contains_forbidden_patterns;
use anyhow::{Context, Result};
use assay_common::limits::{LimitKind, LimitReader};
use sha2::{Digest, Sha256};
use std::io::Read;

/// Result of bundle verification: pass/fail plus optional errors and warnings.
#[derive(Debug, Default)]
pub struct VerifyResult {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl VerifyResult {
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    fn fail(&mut self, msg: impl Into<String>) {
        self.errors.push(msg.into());
    }

    fn warn(&mut self, msg: impl Into<String>) {
        self.warnings.push(msg.into());
    }
}

/// Verify a replay bundle: hashes (manifest vs file contents) and secret scan.
///
/// **Hash checks:** For each path in `manifest.files`, the archive must contain that path and
/// its content must match the recorded sha256 and size. Manifest entry missing in archive → error.
/// Size mismatch → error (conformance). Extra files in the archive (not listed in manifest) are
/// **allowed** for compatibility; they are still **scanned** for forbidden patterns (see below).
///
/// **Secret scan (archive-wide):** We scan **all** archive entries (not just manifest.files).
/// - **cassettes/** and **files/:** hard fail if forbidden patterns (secrets, Authorization, sk-*).
///   Rationale: inputs and cassettes are under our control; they must be safe to share. Extra
///   files under these prefixes are scanned and fail if they contain secrets (no bypass).
/// - **outputs/:** warn only. Outputs can contain user-provided or tool output; we avoid
///   false-positive hard fails.
pub fn verify_bundle<R: Read>(r: R) -> Result<VerifyResult> {
    verify_bundle_with_limits(r, ReplayLimits::default())
}

/// Read once under an explicit ceiling and verify what was read.
pub fn verify_bundle_with_limits<R: Read>(r: R, limits: ReplayLimits) -> Result<VerifyResult> {
    let read = read_bundle_tar_gz_with_limits(r, limits).context("read bundle")?;
    verify_read_bundle(&read)
}

/// One bounded snapshot of a replay bundle: the bytes, what they parsed to, and the verdict.
///
/// `source_digest` is computed over exactly the compressed bytes that produced `read`, which is
/// the point of returning them together. A caller that digests the path and then opens it again
/// publishes a digest describing one snapshot while replaying another.
#[derive(Debug)]
pub struct VerifiedBundle {
    /// Parsed bundle, from the same bytes the digest covers.
    pub read: ReadBundle,
    /// Verification verdict for exactly that `read`.
    pub verify: VerifyResult,
    /// `sha256:<hex>` over the compressed source bytes.
    pub source_digest: String,
}

/// Read, digest and verify a replay bundle from a single bounded snapshot.
///
/// This is the entrypoint a caller should reach for. It takes one read of the source, bounded by
/// `limits.max_source_bytes` before anything is materialized, and binds three things that must
/// agree to that one snapshot: the digest published as provenance, the bundle that is parsed, and
/// the verdict. Digesting a path and separately opening it leaves a window in which those three
/// describe different bytes.
pub fn read_verify_bounded<R: Read>(
    r: R,
    limits: ReplayLimits,
) -> Result<VerifiedBundle, SnapshotError> {
    let mut source = Vec::new();
    LimitReader::new(r, limits.max_source_bytes, LimitKind::SourceBytes)
        .read_to_end(&mut source)
        .map_err(|err| {
            // Classify before wrapping. `.context` on the raw io error would bury the typed cause
            // under an anyhow layer, so the recommended entrypoint would return a weaker contract
            // than the reader it is meant to replace.
            let e = classify_source_ceiling(&err)
                .map(anyhow::Error::from)
                .unwrap_or_else(|| anyhow::Error::from(err).context("read bundle source"));
            // No digest: the source itself could not be read, so there is nothing to attest.
            SnapshotError {
                source_digest: None,
                error: e,
            }
        })?;

    // From here the source is known, so every later failure can still name the bytes it happened
    // on. Losing the digest to "sha256:unknown" on a parse error discards provenance we already
    // hold about the exact input that failed.
    let source_digest = format!("sha256:{}", hex::encode(Sha256::digest(&source)));

    let read =
        read_bundle_tar_gz_with_limits(std::io::Cursor::new(&source), limits).map_err(|error| {
            SnapshotError {
                source_digest: Some(source_digest.clone()),
                error,
            }
        })?;
    let verify = verify_read_bundle(&read).map_err(|error| SnapshotError {
        source_digest: Some(source_digest.clone()),
        error,
    })?;

    Ok(VerifiedBundle {
        read,
        verify,
        source_digest,
    })
}

/// A bounded-snapshot failure, carrying the digest when the source was read.
#[derive(Debug)]
pub struct SnapshotError {
    /// `sha256:` over the source, present whenever the source itself was read successfully.
    pub source_digest: Option<String>,
    pub error: anyhow::Error,
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.error)
    }
}

impl SnapshotError {
    /// The typed ingest refusal behind this failure, if it was one.
    pub fn ingest_refusal(&self) -> Option<&crate::replay::bundle::ReplayIngestError> {
        self.error.downcast_ref()
    }
}

/// Verify an already-read bundle.
///
/// Crate-internal on purpose. Exposed, it invites a caller to verify one `ReadBundle` and act on
/// another; [`read_verify_bounded`] is the shape that cannot be held that way.
pub(crate) fn verify_read_bundle(read: &ReadBundle) -> Result<VerifyResult> {
    let ReadBundle { manifest, entries } = read;
    let mut result = VerifyResult::default();
    let file_manifest = manifest.files.as_ref();

    // Build map path -> data for hash check
    let entry_map: std::collections::BTreeMap<_, _> = entries.iter().cloned().collect();

    if let Some(files) = file_manifest {
        for (path, expected) in files {
            let data = match entry_map.get(path) {
                Some(d) => d,
                None => {
                    result.fail(format!(
                        "manifest lists {} but file missing in bundle",
                        path
                    ));
                    continue;
                }
            };
            let expected_hash = expected.sha256.trim_start_matches("sha256:");
            let actual = hex::encode(Sha256::digest(data));
            if expected_hash != actual {
                result.fail(format!(
                    "hash mismatch for {}: manifest {} vs computed {}",
                    path, expected.sha256, actual
                ));
            }
            if data.len() as u64 != expected.size {
                result.fail(format!(
                    "size mismatch for {}: manifest {} vs actual {}",
                    path,
                    expected.size,
                    data.len()
                ));
            }
        }
    }

    for (path, data) in entries.iter() {
        let has_forbidden = contains_forbidden_patterns(data);
        if path.starts_with(paths::CASSETTES_PREFIX) || path.starts_with(paths::FILES_PREFIX) {
            if has_forbidden {
                result.fail(format!(
                    "forbidden pattern (secret/token) in {}: bundle not safe to share",
                    path
                ));
            }
        } else if path.starts_with(paths::OUTPUTS_PREFIX) && has_forbidden {
            result.warn(format!(
                "output {} may contain secret/token patterns; review before sharing",
                path
            ));
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::bundle::{build_file_manifest, write_bundle_tar_gz, BundleEntry};
    use crate::replay::manifest::ReplayManifest;

    #[test]
    fn verify_clean_bundle_passes() {
        let manifest = ReplayManifest::minimal("2.15.0".into());
        let entries = vec![
            BundleEntry {
                path: "files/trace.jsonl".into(),
                data: b"[]".to_vec(),
            },
            BundleEntry {
                path: "outputs/run.json".into(),
                data: b"{}".to_vec(),
            },
        ];
        let file_manifest = build_file_manifest(&entries).unwrap();
        let mut m = manifest.clone();
        m.files = Some(file_manifest);
        let mut buf = Vec::new();
        write_bundle_tar_gz(&mut buf, &m, &entries).unwrap();
        let result = verify_bundle(std::io::Cursor::new(&buf)).unwrap();
        assert!(result.is_ok(), "errors: {:?}", result.errors);
    }

    #[test]
    fn verify_fails_when_cassette_has_secret() {
        let manifest = ReplayManifest::minimal("2.15.0".into());
        let entries = vec![BundleEntry {
            path: "cassettes/req.json".into(),
            data: b"Authorization: Bearer sk-secret123\n{}".to_vec(),
        }];
        let file_manifest = build_file_manifest(&entries).unwrap();
        let mut m = manifest;
        m.files = Some(file_manifest);
        let mut buf = Vec::new();
        write_bundle_tar_gz(&mut buf, &m, &entries).unwrap();
        let result = verify_bundle(std::io::Cursor::new(&buf)).unwrap();
        assert!(!result.is_ok());
        assert!(result
            .errors
            .iter()
            .any(|e| e.contains("cassettes/") && e.contains("forbidden")));
    }

    #[test]
    fn verify_warns_on_output_with_secret() {
        let manifest = ReplayManifest::minimal("2.15.0".into());
        let entries = vec![BundleEntry {
            path: "outputs/run.json".into(),
            data: b"{\"token\":\"sk-abcdefghij1234567890xyz\"}".to_vec(),
        }];
        let file_manifest = build_file_manifest(&entries).unwrap();
        let mut m = manifest;
        m.files = Some(file_manifest);
        let mut buf = Vec::new();
        write_bundle_tar_gz(&mut buf, &m, &entries).unwrap();
        let result = verify_bundle(std::io::Cursor::new(&buf)).unwrap();
        assert!(
            result.is_ok(),
            "outputs should not hard-fail: {:?}",
            result.errors
        );
        assert!(result.warnings.iter().any(|w| w.contains("outputs/")));
    }

    /// Bundle built with scrubbed cassette content passes verify (safe to share).
    #[test]
    fn verify_passes_when_cassette_was_scrubbed() {
        let raw_cassette = b"Authorization: Bearer sk-secret123\n{}";
        let scrubbed = crate::replay::scrub::scrub_content(raw_cassette);
        let manifest = ReplayManifest::minimal("2.15.0".into());
        let entries = vec![BundleEntry {
            path: "cassettes/req.json".into(),
            data: scrubbed,
        }];
        let file_manifest = build_file_manifest(&entries).unwrap();
        let mut m = manifest;
        m.files = Some(file_manifest);
        let mut buf = Vec::new();
        write_bundle_tar_gz(&mut buf, &m, &entries).unwrap();
        let result = verify_bundle(std::io::Cursor::new(&buf)).unwrap();
        assert!(
            result.is_ok(),
            "scrubbed bundle should pass: {:?}",
            result.errors
        );
    }

    #[test]
    fn verify_fails_when_files_has_secret() {
        let manifest = ReplayManifest::minimal("2.15.0".into());
        let entries = vec![BundleEntry {
            path: "files/config.yaml".into(),
            data: b"api_key: sk-abcdefghij1234567890abcdefghij".to_vec(),
        }];
        let file_manifest = build_file_manifest(&entries).unwrap();
        let mut m = manifest;
        m.files = Some(file_manifest);
        let mut buf = Vec::new();
        write_bundle_tar_gz(&mut buf, &m, &entries).unwrap();
        let result = verify_bundle(std::io::Cursor::new(&buf)).unwrap();
        assert!(!result.is_ok());
        assert!(result
            .errors
            .iter()
            .any(|e| e.contains("files/") && e.contains("forbidden")));
    }

    /// Extra file under cassettes/ with secret but NOT in manifest.files → verify fails (no bypass).
    #[test]
    fn verify_fails_when_extra_cassette_has_secret() {
        let manifest = ReplayManifest::minimal("2.15.0".into());
        let entries = vec![
            BundleEntry {
                path: "files/trace.jsonl".into(),
                data: b"[]".to_vec(),
            },
            BundleEntry {
                path: "cassettes/extra.txt".into(),
                data: b"Authorization: Bearer SECRET\n".to_vec(),
            },
        ];
        let file_manifest = build_file_manifest(&[entries[0].clone()]).unwrap();
        let mut m = manifest;
        m.files = Some(file_manifest);
        let mut buf = Vec::new();
        write_bundle_tar_gz(&mut buf, &m, &entries).unwrap();
        let result = verify_bundle(std::io::Cursor::new(&buf)).unwrap();
        assert!(
            !result.is_ok(),
            "extra cassettes/ file with secret must fail: {:?}",
            result.errors
        );
        assert!(result
            .errors
            .iter()
            .any(|e| e.contains("cassettes/") && e.contains("forbidden")));
    }

    /// Extra files in archive (not in manifest.files) without secrets are allowed; verify passes.
    #[test]
    fn verify_allows_extra_files_in_archive() {
        let manifest = ReplayManifest::minimal("2.15.0".into());
        let entries = vec![
            BundleEntry {
                path: "files/trace.jsonl".into(),
                data: b"[]".to_vec(),
            },
            BundleEntry {
                path: "outputs/extra.json".into(),
                data: b"{}".to_vec(),
            },
        ];
        let file_manifest = build_file_manifest(&[entries[0].clone()]).unwrap();
        let mut m = manifest;
        m.files = Some(file_manifest);
        let mut buf = Vec::new();
        write_bundle_tar_gz(&mut buf, &m, &entries).unwrap();
        let result = verify_bundle(std::io::Cursor::new(&buf)).unwrap();
        assert!(
            result.is_ok(),
            "extra file outputs/extra.json should be allowed: {:?}",
            result.errors
        );
    }

    #[test]
    fn verify_fails_when_manifest_entry_missing_in_archive() {
        let manifest = ReplayManifest::minimal("2.15.0".into());
        let entries = vec![BundleEntry {
            path: "files/trace.jsonl".into(),
            data: b"[]".to_vec(),
        }];
        let mut file_manifest = build_file_manifest(&entries).unwrap();
        file_manifest.insert(
            "files/missing.jsonl".to_string(),
            crate::replay::manifest::FileManifestEntry {
                sha256: "sha256:ab".to_string(),
                size: 0,
                mode: None,
                content_type: None,
            },
        );
        let mut m = manifest;
        m.files = Some(file_manifest);
        let mut buf = Vec::new();
        write_bundle_tar_gz(&mut buf, &m, &entries).unwrap();
        let result = verify_bundle(std::io::Cursor::new(&buf)).unwrap();
        assert!(!result.is_ok());
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("missing in bundle")),
            "{:?}",
            result.errors
        );
    }

    #[test]
    fn verify_fails_on_hash_mismatch() {
        let manifest = ReplayManifest::minimal("2.15.0".into());
        let entries = vec![BundleEntry {
            path: "files/trace.jsonl".into(),
            data: b"[]".to_vec(),
        }];
        let mut file_manifest = build_file_manifest(&entries).unwrap();
        // Corrupt the hash in manifest
        file_manifest.get_mut("files/trace.jsonl").unwrap().sha256 = "sha256:deadbeef".into();
        let mut m = manifest;
        m.files = Some(file_manifest);
        let mut buf = Vec::new();
        write_bundle_tar_gz(&mut buf, &m, &entries).unwrap();
        let result = verify_bundle(std::io::Cursor::new(&buf)).unwrap();
        assert!(!result.is_ok());
        assert!(result.errors.iter().any(|e| e.contains("hash mismatch")));
    }

    #[test]
    fn verify_fails_on_size_mismatch() {
        let manifest = ReplayManifest::minimal("2.15.0".into());
        let entries = vec![BundleEntry {
            path: "files/trace.jsonl".into(),
            data: b"[]".to_vec(),
        }];
        let mut file_manifest = build_file_manifest(&entries).unwrap();
        file_manifest.get_mut("files/trace.jsonl").unwrap().size = 999;
        let mut m = manifest;
        m.files = Some(file_manifest);
        let mut buf = Vec::new();
        write_bundle_tar_gz(&mut buf, &m, &entries).unwrap();
        let result = verify_bundle(std::io::Cursor::new(&buf)).unwrap();
        assert!(!result.is_ok());
        assert!(result.errors.iter().any(|e| e.contains("size mismatch")));
    }
}
