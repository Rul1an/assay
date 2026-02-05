//! Replay bundle container writer (E9).
//!
//! Writes a hermetic .tar.gz with canonical layout: manifest.json, then
//! files under files/, outputs/, cassettes/ in deterministic order.
//! No user-facing CLI here (E9c); this is the core library for bundle creation.

use crate::replay::manifest::ReplayManifest;
use anyhow::{Context, Result};
use flate2::Compression;
use flate2::GzBuilder;
use serde_json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use tar::{Builder, Header};

/// Canonical paths inside the bundle (POSIX, relative to root).
pub mod paths {
    /// Manifest at bundle root.
    pub const MANIFEST: &str = "manifest.json";
    /// Inputs (config, trace, etc.).
    pub const FILES_PREFIX: &str = "files/";
    /// Outputs (run.json, summary.json, sarif, junit).
    pub const OUTPUTS_PREFIX: &str = "outputs/";
    /// Scrubbed VCR/cassettes.
    pub const CASSETTES_PREFIX: &str = "cassettes/";
}

/// Single file to add to the bundle: relative path (POSIX) and contents.
#[derive(Debug, Clone)]
pub struct BundleEntry {
    /// Relative path with POSIX forward slashes (e.g. "files/trace.jsonl").
    pub path: String,
    /// File contents.
    pub data: Vec<u8>,
}

/// Write a replay bundle to `w` as .tar.gz: manifest first, then entries in sorted order.
/// Uses deterministic tar headers (mtime 0, fixed mode) for reproducible archives.
pub fn write_bundle_tar_gz<W: Write>(
    w: W,
    manifest: &ReplayManifest,
    entries: &[BundleEntry],
) -> Result<()> {
    let manifest_json = serde_json::to_vec(manifest).context("serialize manifest")?;

    let gz = GzBuilder::new().mtime(0).write(w, Compression::default());
    let mut tar = Builder::new(gz);
    tar.mode(tar::HeaderMode::Deterministic);

    write_tar_entry(&mut tar, paths::MANIFEST, &manifest_json)?;

    let mut sorted: Vec<_> = entries.iter().collect();
    sorted.sort_by(|a, b| a.path.as_str().cmp(b.path.as_str()));

    for e in &sorted {
        normalize_path_and_append(&mut tar, &e.path, &e.data)?;
    }

    let gz = tar.into_inner().context("finalize tar")?;
    gz.finish().context("finish gzip")?;
    Ok(())
}

/// Compute SHA256 of the entire archive (for provenance.bundle_digest).
/// Caller must pass the same manifest + entries in the same order as write_bundle_tar_gz.
pub fn bundle_digest(manifest: &ReplayManifest, entries: &[BundleEntry]) -> Result<String> {
    let mut buf = Vec::new();
    write_bundle_tar_gz(&mut buf, manifest, entries)?;
    let hash = Sha256::digest(&buf);
    Ok(hex::encode(hash))
}

fn write_tar_entry<T: Write>(tar: &mut Builder<T>, path: &str, data: &[u8]) -> Result<()> {
    let mut header = Header::new_gnu();
    header.set_path(path).context("set_path")?;
    header.set_size(data.len() as u64);
    header.set_mode(0o644);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_cksum();
    tar.append(&header, data).context("append entry")?;
    Ok(())
}

/// Validates and normalizes a bundle **entry** path. Fail-closed: returns Ok(normalized) or Err.
/// Applies only to entry paths (files under files/, outputs/, cassettes/). The manifest file
/// (`manifest.json`) is written via [write_tar_entry] with [paths::MANIFEST] and never goes
/// through this validator.
///
/// Rules: POSIX (backslash → slash, no leading slash); no empty path or empty segments (e.g.
/// `files//x` rejected); no segment "." or ".." (segment check, so `files/a..b.txt` is allowed);
/// no drive letter (':' in first segment); canonical prefix required: files/, outputs/, or cassettes/.
fn validate_entry_path(path: &str) -> Result<String> {
    let normalized = path.replace('\\', "/").trim_start_matches('/').to_string();
    if normalized.is_empty() {
        anyhow::bail!("invalid bundle path: empty path");
    }
    let segments: Vec<&str> = normalized.split('/').collect();
    if segments[0].contains(':') {
        anyhow::bail!(
            "invalid bundle path: drive-letter or ':' in first segment (path: {})",
            path
        );
    }
    for seg in &segments {
        if seg.is_empty() {
            anyhow::bail!("invalid bundle path: empty segment (path: {})", path);
        }
        if *seg == "." || *seg == ".." {
            anyhow::bail!(
                "invalid bundle path: traversal segment '.' or '..' (path: {})",
                path
            );
        }
    }
    let has_canonical_prefix = normalized.starts_with(paths::FILES_PREFIX)
        || normalized.starts_with(paths::OUTPUTS_PREFIX)
        || normalized.starts_with(paths::CASSETTES_PREFIX);
    if !has_canonical_prefix {
        anyhow::bail!(
            "invalid bundle path prefix: must be files/, outputs/, or cassettes/ (path: {})",
            path
        );
    }
    Ok(normalized)
}

/// Normalize path (validate by segment + canonical prefix), then append to tar.
fn normalize_path_and_append<T: Write>(
    tar: &mut Builder<T>,
    path: &str,
    data: &[u8],
) -> Result<()> {
    let normalized = validate_entry_path(path)?;
    write_tar_entry(tar, &normalized, data)
}

/// Build a file manifest (path -> FileManifestEntry) from entries. Fail-closed: invalid path → Error
/// (same policy as writer). Paths must be valid and under files/, outputs/, or cassettes/.
pub fn build_file_manifest(
    entries: &[BundleEntry],
) -> Result<BTreeMap<String, crate::replay::manifest::FileManifestEntry>> {
    let mut out = BTreeMap::new();
    for e in entries {
        let path = validate_entry_path(&e.path)?;
        let hash = Sha256::digest(&e.data);
        out.insert(
            path.clone(),
            crate::replay::manifest::FileManifestEntry {
                sha256: format!("sha256:{}", hex::encode(hash)),
                size: e.data.len() as u64,
                mode: Some(0o644),
                content_type: content_type_hint(Path::new(&path)),
            },
        );
    }
    Ok(out)
}

fn content_type_hint(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?;
    Some(match ext {
        "json" => "application/json".to_string(),
        "jsonl" => "application/x-ndjson".to_string(),
        "xml" => "application/xml".to_string(),
        "yaml" | "yml" => "application/x-yaml".to_string(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::manifest::{
        ReplayCoverage, ReplayManifest, ReplayOutputs, ReplaySeeds, ScrubPolicy,
    };
    use std::collections::BTreeMap;

    #[test]
    fn write_bundle_minimal_roundtrip() {
        let manifest = ReplayManifest::minimal("2.15.0".into());
        let entries = vec![BundleEntry {
            path: "outputs/summary.json".into(),
            data: br#"{"schema_version":1}"#.to_vec(),
        }];
        let mut buf = Vec::new();
        write_bundle_tar_gz(&mut buf, &manifest, &entries).unwrap();
        assert!(!buf.is_empty());
        let digest = bundle_digest(&manifest, &entries).unwrap();
        assert_eq!(digest.len(), 64);
    }

    #[test]
    fn build_file_manifest_normalizes_paths() {
        let entries = vec![BundleEntry {
            path: "files/trace.jsonl".into(),
            data: vec![1, 2, 3],
        }];
        let manifest_map = build_file_manifest(&entries).unwrap();
        assert_eq!(manifest_map.len(), 1);
        let entry = manifest_map.get("files/trace.jsonl").unwrap();
        assert_eq!(entry.size, 3);
        assert!(entry.sha256.starts_with("sha256:"));
    }

    /// Legitimate filename with ".." in segment (not traversal) is allowed.
    #[test]
    fn path_segment_dotdot_allows_literal_dotdot_in_filename() {
        let manifest = ReplayManifest::minimal("2.15.0".into());
        let entries = vec![BundleEntry {
            path: "files/a..b.txt".into(),
            data: b"ok".to_vec(),
        }];
        let mut buf = Vec::new();
        write_bundle_tar_gz(&mut buf, &manifest, &entries).unwrap();
        let names = list_tar_gz_paths(&buf);
        assert!(names.contains(&"files/a..b.txt".to_string()));
    }

    /// Non-canonical prefix (evil.txt, x/y) rejected.
    #[test]
    fn path_must_have_canonical_prefix() {
        let manifest = ReplayManifest::minimal("2.15.0".into());
        for bad in ["evil.txt", "x/y/z", "output/run.json"] {
            let entries = vec![BundleEntry {
                path: bad.to_string(),
                data: vec![],
            }];
            let err = write_bundle_tar_gz(&mut Vec::new(), &manifest, &entries).unwrap_err();
            assert!(
                err.to_string().contains("invalid bundle path prefix"),
                "{}",
                bad
            );
        }
    }

    /// Empty segment (duplicate slash) rejected.
    #[test]
    fn path_rejects_empty_segment() {
        let manifest = ReplayManifest::minimal("2.15.0".into());
        let entries = vec![BundleEntry {
            path: "files//x.json".into(),
            data: vec![],
        }];
        let err = write_bundle_tar_gz(&mut Vec::new(), &manifest, &entries).unwrap_err();
        assert!(err.to_string().contains("empty segment"), "files//x");
    }

    /// Windows drive-letter-like path rejected.
    #[test]
    fn path_rejects_drive_letter() {
        let manifest = ReplayManifest::minimal("2.15.0".into());
        for bad in ["C:/foo", "C:\\foo", "D:bar"] {
            let entries = vec![BundleEntry {
                path: bad.to_string(),
                data: vec![],
            }];
            let err = write_bundle_tar_gz(&mut Vec::new(), &manifest, &entries).unwrap_err();
            assert!(
                err.to_string().contains("drive-letter")
                    || err.to_string().contains("first segment"),
                "{}",
                bad
            );
        }
    }

    /// build_file_manifest fail-closed: invalid path returns Err (same policy as writer).
    #[test]
    fn build_file_manifest_fail_closed_on_invalid_path() {
        let entries = vec![
            BundleEntry {
                path: "files/ok.json".into(),
                data: vec![],
            },
            BundleEntry {
                path: "../secrets.txt".into(),
                data: vec![],
            },
        ];
        let err = build_file_manifest(&entries).unwrap_err();
        assert!(err.to_string().contains("invalid bundle path"));
    }

    /// Audit: digest of written bytes equals bundle_digest(manifest, entries).
    #[test]
    fn bundle_digest_equals_sha256_of_written_bytes() {
        let manifest = ReplayManifest::minimal("2.15.0".into());
        let entries = vec![
            BundleEntry {
                path: "files/trace.jsonl".into(),
                data: b"[]".to_vec(),
            },
            BundleEntry {
                path: "outputs/summary.json".into(),
                data: b"{}".to_vec(),
            },
        ];
        let mut buf = Vec::new();
        write_bundle_tar_gz(&mut buf, &manifest, &entries).unwrap();
        let digest_from_fn = bundle_digest(&manifest, &entries).unwrap();
        let hash_of_bytes = hex::encode(Sha256::digest(&buf));
        assert_eq!(
            digest_from_fn, hash_of_bytes,
            "bundle_digest must equal sha256(written bytes)"
        );
    }

    /// Audit: path traversal (..) and empty path rejected; no .. or absolute in output.
    #[test]
    fn path_traversal_rejected_and_output_has_no_traversal() {
        let manifest = ReplayManifest::minimal("2.15.0".into());
        for bad_path in [
            "../secrets.txt",
            "files/../../etc/passwd",
            "outputs/../leak",
            "",
        ] {
            let entries = vec![BundleEntry {
                path: bad_path.to_string(),
                data: vec![],
            }];
            let mut buf = Vec::new();
            let err = write_bundle_tar_gz(&mut buf, &manifest, &entries).unwrap_err();
            assert!(
                err.to_string().contains("invalid bundle path"),
                "{}",
                bad_path
            );
        }
        // Leading slash and backslash are normalized; result must not be in archive as absolute/traversal
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
        let mut buf = Vec::new();
        write_bundle_tar_gz(&mut buf, &manifest, &entries).unwrap();
        let names = list_tar_gz_paths(&buf);
        for name in &names {
            assert!(!name.contains(".."), "no .. in archive path: {}", name);
            assert!(
                !name.starts_with('/'),
                "no leading / in archive path: {}",
                name
            );
        }
        assert!(names.iter().any(|s| s == "manifest.json"));
        assert!(names.iter().any(|s| s.starts_with("files/")));
        assert!(names.iter().any(|s| s.starts_with("outputs/")));
    }

    /// Audit: full manifest (replay_coverage, seeds, scrub_policy) and canonical layout.
    #[test]
    fn audit_full_manifest_and_canonical_layout() {
        let mut reason = BTreeMap::new();
        reason.insert(
            "test_b".to_string(),
            "judge response not cached".to_string(),
        );
        let manifest = ReplayManifest {
            schema_version: 1,
            assay_version: "2.15.0".to_string(),
            created_at: Some("2025-01-27T12:00:00Z".to_string()),
            source_run_path: Some(".assay/run_abc123".to_string()),
            git_sha: Some("a1b2c3d4e5f6".to_string()),
            git_dirty: Some(false),
            workflow_run_id: None,
            config_digest: None,
            policy_digest: None,
            baseline_digest: None,
            trace_digest: None,
            trace_path: Some("files/trace.jsonl".to_string()),
            outputs: Some(ReplayOutputs {
                run: Some("outputs/run.json".to_string()),
                summary: Some("outputs/summary.json".to_string()),
                junit: None,
                sarif: None,
            }),
            toolchain: None,
            seeds: Some(ReplaySeeds {
                seed_version: Some(1),
                order_seed: Some("42".to_string()),
                judge_seed: None,
            }),
            replay_coverage: Some(ReplayCoverage {
                complete_tests: vec!["test_a".to_string()],
                incomplete_tests: vec!["test_b".to_string()],
                reason: Some(reason),
            }),
            scrub_policy: Some(ScrubPolicy::default()),
            files: None,
            env: None,
        };
        let entries = vec![
            BundleEntry {
                path: "files/trace.jsonl".into(),
                data: b"[]".to_vec(),
            },
            BundleEntry {
                path: "outputs/run.json".into(),
                data: b"{}".to_vec(),
            },
            BundleEntry {
                path: "outputs/summary.json".into(),
                data: b"{}".to_vec(),
            },
            BundleEntry {
                path: "cassettes/.gitkeep".into(),
                data: vec![],
            },
        ];
        let mut buf = Vec::new();
        write_bundle_tar_gz(&mut buf, &manifest, &entries).unwrap();
        let names = list_tar_gz_paths(&buf);
        assert!(
            names.contains(&"manifest.json".to_string()),
            "canonical: manifest at root"
        );
        assert!(names
            .iter()
            .all(|p| !p.contains("..") && !p.starts_with('/')));
        assert!(names.contains(&"manifest.json".to_string()));
        assert!(names.iter().any(|p| p.starts_with("files/")));
        assert!(names.iter().any(|p| p.starts_with("outputs/")));
        assert!(names.iter().any(|p| p.starts_with("cassettes/")));
    }

    fn list_tar_gz_paths(gz: &[u8]) -> Vec<String> {
        let dec = flate2::read::GzDecoder::new(gz);
        let mut ar = tar::Archive::new(dec);
        let mut names = Vec::new();
        for e in ar.entries().unwrap() {
            let e = e.unwrap();
            let path = e.path().unwrap();
            names.push(path.to_string_lossy().replace('\\', "/"));
        }
        names.sort();
        names
    }
}
