//! Replay bundle verification (E9b).
//!
//! Validates bundle integrity (hashes) and runs secret scan: hard fail for
//! cassettes/ and files/, warn for outputs/. See E9-REPLAY-BUNDLE-PLAN §2.5.

use crate::replay::bundle::{paths, read_bundle_tar_gz, ReadBundle};
use crate::replay::scrub::contains_forbidden_patterns;
use anyhow::{Context, Result};
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
/// its content must match the recorded sha256/size. Manifest entry missing in archive → error.
/// Extra files in the archive (not listed in manifest) are **allowed** (no error or warning).
///
/// **Secret scan (hard fail vs warn):**
/// - **cassettes/** and **files/:** hard fail if forbidden patterns (secrets, Authorization, sk-*).
///   Rationale: inputs and cassettes are under our control; they must be safe to share.
/// - **outputs/:** warn only. Outputs can contain user-provided or tool output (e.g. stacktraces,
///   docs) that may look like tokens; we avoid false-positive hard fails.
pub fn verify_bundle<R: Read>(r: R) -> Result<VerifyResult> {
    let ReadBundle { manifest, entries } = read_bundle_tar_gz(r).context("read bundle")?;
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
                result.warn(format!(
                    "size mismatch for {}: manifest {} vs actual {}",
                    path,
                    expected.size,
                    data.len()
                ));
            }
        }
    }

    for (path, data) in &entries {
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
        let scrubbed = crate::replay::scrub::scrub_content(raw_cassette)
            .into_owned()
            .into_bytes();
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

    /// Extra files in archive (not in manifest.files) are allowed; verify still passes.
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
}
