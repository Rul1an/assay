use super::limits::{
    check_manifest_json_depth, classify_member_ceiling, classify_source_ceiling,
    ReplayContractError, ReplayIngestError, ReplayLimits,
};
use super::{paths, BundleEntry, ReadBundle};
use crate::replay::manifest::ReplayManifest;
use anyhow::{Context, Result};
use assay_common::limits::{LimitKind, LimitReader};
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

/// Read a replay bundle under the default [`ReplayLimits`].
///
/// Kept for existing callers who did not need to name their own budget. New callers should
/// prefer [`read_bundle_tar_gz_with_limits`] so the ceiling is visible at the call site.
pub fn read_bundle_tar_gz<R: Read>(r: R) -> Result<ReadBundle> {
    read_bundle_tar_gz_with_limits(r, ReplayLimits::default())
}

/// Read a replay bundle under an explicit resource ceiling.
///
/// ADR-043 §1: every dimension applies to the source stream before the input is materialised.
/// The compressed source, the gzip expansion, the manifest, each entry body, path length and
/// entry count all have their own ceiling; a ceiling that fails to the caller wrapped in
/// `anyhow::Error` still carries the typed [`ReplayIngestError`] and can be recovered with
/// `downcast_ref`. That is the supported way to classify a refusal; matching on the rendered
/// message is not.
pub fn read_bundle_tar_gz_with_limits<R: Read>(r: R, limits: ReplayLimits) -> Result<ReadBundle> {
    // 1. Snapshot the whole source under the ceiling before parsing anything.
    //
    //    Streaming the ceiling into the tar walker only bounds the prefix gzip and tar choose to
    //    consume. They stop once the archive is logically complete, so a valid bundle followed by
    //    an arbitrary suffix passed a ceiling far below the real input size: the trailing bytes
    //    were never requested and therefore never counted. Reading to EOF through the same
    //    ceiling counts everything the source actually holds.
    let mut snapshot = Vec::new();
    LimitReader::new(r, limits.max_source_bytes, LimitKind::SourceBytes)
        .read_to_end(&mut snapshot)
        .map_err(|err| {
            classify_source_ceiling(&err)
                .map(anyhow::Error::from)
                .unwrap_or_else(|| anyhow::Error::from(err).context("read bundle source"))
        })?;

    // 2. Bound the gzip expansion. A small compressed input can still decode to gigabytes,
    //    which is what makes the source ceiling on its own insufficient.
    let decoder = GzDecoder::new(std::io::Cursor::new(&snapshot));
    let bounded_decoder =
        LimitReader::new(decoder, limits.max_decoded_bytes, LimitKind::DecodedBytes);
    let mut ar = Archive::new(bounded_decoder);

    let mut manifest_data: Option<Vec<u8>> = None;
    let mut seen: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    // The manifest counts against the entry ceiling too: it is one member the archive
    // carries.
    let mut entry_count: usize = 0;

    let entries = ar.entries().map_err(|e| {
        classify_source_ceiling(&e)
            .map(anyhow::Error::from)
            .unwrap_or_else(|| anyhow::Error::from(e).context("list tar entries"))
    })?;

    for entry in entries {
        let mut e = entry.map_err(|err| {
            classify_source_ceiling(&err)
                .map(anyhow::Error::from)
                .unwrap_or_else(|| anyhow::Error::from(err).context("read tar entry"))
        })?;

        entry_count += 1;
        if entry_count > limits.max_entries {
            return Err(anyhow::Error::from(ReplayIngestError::TooManyEntries {
                limit: limits.max_entries,
            }));
        }

        // Bound the path on the bytes the archive actually carries, before any conversion.
        // `to_string_lossy` replaces invalid UTF-8 with U+FFFD, which is three bytes for every
        // one it replaces, so a check on the converted string measures something the archive did
        // not send and can be walked past by a name that is invalid on purpose.
        let raw_path_len = e.path_bytes().len();
        if raw_path_len > limits.max_path_len {
            return Err(anyhow::Error::from(ReplayIngestError::PathTooLong {
                limit: limits.max_path_len,
            }));
        }

        let path = e.path().context("entry path")?;
        let path_str = path.to_string_lossy().replace('\\', "/");

        if path_str == paths::MANIFEST {
            // Refuse before reading, so the manifest that is verified is unambiguously the one
            // the archive declared first. Overwriting on a second entry let an archive show one
            // manifest to whoever inspects the head of the stream and a different one to the
            // verifier, while the non-manifest duplicates were caught and this one was not.
            if manifest_data.is_some() {
                return Err(anyhow::Error::from(ReplayContractError::DuplicateManifest));
            }
            let mut data = Vec::new();
            let mut bounded =
                LimitReader::new(&mut e, limits.max_manifest_bytes, LimitKind::MemberBytes);
            bounded.read_to_end(&mut data).map_err(|err| {
                classify_member_ceiling(&err)
                    .map(anyhow::Error::from)
                    .unwrap_or_else(|| anyhow::Error::from(err).context("read manifest body"))
            })?;
            check_manifest_json_depth(&data, limits.max_manifest_json_depth)?;
            manifest_data = Some(data);
            continue;
        }

        // Path validation (segments, prefix) runs after the length check, so an oversized name
        // is refused before it is normalised and cloned into a map key. It is not a claim about
        // all allocation: the tar reader has already parsed the header, and a PAX extended name
        // is materialized by that layer before either check sees it.
        // Use what the validator returns, never the string that was handed to it. The validator
        // trims leading slashes as part of normalising, so `/files/x` passes as `files/x` — and
        // storing the raw name meant the value that was checked and the value that was kept were
        // two different strings. The consumer does `workspace.join(rel)`, and joining an absolute
        // path discards the workspace prefix entirely, so that gap materialized entries at the
        // filesystem root. Validating one form and keeping another is the defect; the canonical
        // path is the only form allowed past this point.
        let canonical = paths::validate_entry_path(&path_str)?;

        let mut data = Vec::new();
        let mut bounded = LimitReader::new(&mut e, limits.max_member_bytes, LimitKind::MemberBytes);
        bounded.read_to_end(&mut data).map_err(|err| {
            classify_member_ceiling(&err)
                .map(anyhow::Error::from)
                .unwrap_or_else(|| anyhow::Error::from(err).context("read entry body"))
        })?;

        // Detection is on the canonical form too, so two spellings of one path collide instead of
        // producing two entries that a consumer resolves to the same file.
        if seen.insert(canonical, data).is_some() {
            return Err(anyhow::Error::from(ReplayContractError::DuplicatePath));
        }
    }

    let manifest_json = manifest_data.context("manifest.json missing in bundle")?;
    let manifest: ReplayManifest =
        serde_json::from_slice(&manifest_json).context("parse manifest.json")?;
    let entries = seen.into_iter().collect();
    Ok(ReadBundle { manifest, entries })
}
