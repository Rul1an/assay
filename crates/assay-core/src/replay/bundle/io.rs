use super::{paths, BundleEntry, ReadBundle};
use crate::replay::manifest::ReplayManifest;
use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use flate2::Compression;
use flate2::GzBuilder;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use tar::{Archive, Builder, Header};

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

/// Normalize path (validate by segment + canonical prefix), then append to tar.
fn normalize_path_and_append<T: Write>(
    tar: &mut Builder<T>,
    path: &str,
    data: &[u8],
) -> Result<()> {
    let normalized = paths::validate_entry_path(path)?;
    write_tar_entry(tar, &normalized, data)
}

/// Read a replay bundle from .tar.gz: parse manifest and collect all entry (path, data).
/// Paths normalized to POSIX. Enforces same path policy as writer: only manifest.json or
/// files/, outputs/, cassettes/ (no empty segment, no . or .., no drive letter). Duplicate
/// paths in tar -> Error. Missing manifest.json -> Error.
pub fn read_bundle_tar_gz<R: Read>(r: R) -> Result<ReadBundle> {
    let dec = GzDecoder::new(r);
    let mut ar = Archive::new(dec);
    let mut manifest_data: Option<Vec<u8>> = None;
    let mut seen = BTreeMap::new();
    for entry in ar.entries().context("list tar entries")? {
        let mut e = entry.context("read tar entry")?;
        let path = e.path().context("entry path")?;
        let path_str = path.to_string_lossy().replace('\\', "/");
        if path_str == paths::MANIFEST {
            let mut data = Vec::new();
            e.read_to_end(&mut data).context("read manifest body")?;
            manifest_data = Some(data);
            continue;
        }
        paths::validate_entry_path(&path_str)?;
        let mut data = Vec::new();
        e.read_to_end(&mut data).context("read entry body")?;
        if seen.insert(path_str.clone(), data).is_some() {
            anyhow::bail!("duplicate path in bundle: {}", path_str);
        }
    }
    let manifest_json = manifest_data.context("manifest.json missing in bundle")?;
    let manifest: ReplayManifest =
        serde_json::from_slice(&manifest_json).context("parse manifest.json")?;
    let entries = seen.into_iter().collect();
    Ok(ReadBundle { manifest, entries })
}
