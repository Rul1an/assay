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

/// Normalize path to POSIX relative (forward slashes, no leading slash) and append.
fn normalize_path_and_append<T: Write>(
    tar: &mut Builder<T>,
    path: &str,
    data: &[u8],
) -> Result<()> {
    let normalized = path.replace('\\', "/").trim_start_matches('/').to_string();
    if normalized.is_empty() || normalized.contains("..") {
        anyhow::bail!("invalid bundle path: {}", path);
    }
    write_tar_entry(tar, &normalized, data)
}

/// Build a file manifest (path -> FileManifestEntry) from entries.
/// Paths are normalized to POSIX. Call after you have all entry data to compute sha256/size.
pub fn build_file_manifest(
    entries: &[BundleEntry],
) -> BTreeMap<String, crate::replay::manifest::FileManifestEntry> {
    let mut out = BTreeMap::new();
    for e in entries {
        let path = e
            .path
            .replace('\\', "/")
            .trim_start_matches('/')
            .to_string();
        if path.is_empty() || path.contains("..") {
            continue;
        }
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
    out
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
    use crate::replay::manifest::ReplayManifest;

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
        let manifest_map = build_file_manifest(&entries);
        assert_eq!(manifest_map.len(), 1);
        let entry = manifest_map.get("files/trace.jsonl").unwrap();
        assert_eq!(entry.size, 3);
        assert!(entry.sha256.starts_with("sha256:"));
    }
}
