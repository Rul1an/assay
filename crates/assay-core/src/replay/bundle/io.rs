use super::limits::{
    check_manifest_json_depth, classify_member_ceiling, classify_source_ceiling,
    ReplayContractError, ReplayIngestError, ReplayLimits,
};
use super::{paths, BundleEntry, ReadBundle};
use crate::replay::manifest::ReplayManifest;
use anyhow::{Context, Result};
use assay_common::limits::{LimitKind, LimitReader};
use flate2::bufread::GzDecoder;
use flate2::Compression;
use flate2::GzBuilder;
use std::cell::Cell;
use std::collections::BTreeMap;
use std::io::{BufRead, Read, Write};
use std::rc::Rc;
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

/// The snapshot, read through `BufRead` so the reader knows how much of it gzip consumed.
///
/// Mirrors `ConsumedSlice` in `assay-evidence`'s bundle verifier; the two crates share no private
/// code, so the behaviour is pinned by the same differential cases in both test suites.
/// `flate2::bufread::GzDecoder` decodes one member and consumes nothing past it, so after the
/// archive is drained any byte beyond `consumed` was never decoded here.
struct ConsumedSlice<'a> {
    data: &'a [u8],
    consumed: Rc<Cell<usize>>,
}

impl Read for ConsumedSlice<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let start = self.consumed.get();
        let rest = &self.data[start..];
        let n = rest.len().min(buf.len());
        buf[..n].copy_from_slice(&rest[..n]);
        self.consumed.set(start + n);
        Ok(n)
    }
}

impl BufRead for ConsumedSlice<'_> {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        Ok(&self.data[self.consumed.get()..])
    }

    fn consume(&mut self, amt: usize) {
        let end = (self.consumed.get() + amt).min(self.data.len());
        self.consumed.set(end);
    }
}

/// Whether a member's raw header is a plain file header, on which tar readers cannot differ.
///
/// Mirrors `plain_member_violation` in `assay-evidence`'s bundle verifier, pinned by the same
/// differential cases. Readers disagree on which of two PAX records wins, on what a malformed one
/// falls back to, on whether a GNU header's bytes at offset 345 are a name prefix, and on whether
/// a size field with a separator in it is a number. So only the headers the writer emits pass:
/// a regular file (`'0'` or the old-style `'\0'` the writer uses), no link name, nothing from the
/// name prefix on, a name without a trailing slash (a directory to other readers), and a size of
/// eleven octal digits ended by a NUL or a space, as POSIX writes it.
fn is_plain_member(header: &Header) -> bool {
    let bytes = header.as_bytes();
    let size = &bytes[124..136];
    matches!(bytes[156], b'0' | b'\0')
        && bytes[157..257].iter().chain(&bytes[345..]).all(|&b| b == 0)
        && !header.path_bytes().ends_with(b"/")
        && size[..11].iter().all(|b| (b'0'..=b'7').contains(b))
        && matches!(size[11], b'\0' | b' ')
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
    let consumed = Rc::new(Cell::new(0usize));
    let decoder = GzDecoder::new(ConsumedSlice {
        data: &snapshot,
        consumed: Rc::clone(&consumed),
    });
    let bounded_decoder =
        LimitReader::new(decoder, limits.max_decoded_bytes, LimitKind::DecodedBytes);
    let mut ar = Archive::new(bounded_decoder);

    let mut manifest_data: Option<Vec<u8>> = None;
    let mut seen: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    // The manifest counts against the entry ceiling too: it is one member the archive
    // carries.
    let mut entry_count: usize = 0;

    // Raw, so the tar crate applies no PAX or GNU extension record and every member is exactly
    // what its own header says; `is_plain_member` then refuses the records themselves.
    let entries = ar
        .entries()
        .map_err(|e| {
            classify_source_ceiling(&e)
                .map(anyhow::Error::from)
                .unwrap_or_else(|| anyhow::Error::from(e).context("list tar entries"))
        })?
        .raw(true);

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

        if !is_plain_member(e.header()) {
            return Err(anyhow::Error::from(ReplayContractError::NotAPlainMember));
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

        // One name, not two. Rendering the name lossily replaced invalid UTF-8 with U+FFFD, and
        // rewriting a backslash into a separator stored the member under a name no other reader
        // reports for it. Both are refused instead, so the name this reader uses is the name the
        // archive carries.
        let raw_path = e.path_bytes().into_owned();
        let path_str = std::str::from_utf8(&raw_path)
            .ok()
            .filter(|name| !name.contains('\\'))
            .ok_or(ReplayContractError::AmbiguousMemberName)?
            .to_string();

        if path_str == paths::MANIFEST {
            // Refuse on meeting a second manifest, before its body is read and before it can
            // replace the first, so the manifest that is verified is unambiguously the one the
            // archive declared first. Overwriting on a second entry let an archive show one
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
        // all allocation: the tar reader has already parsed the header before either check runs.
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

    // Drain what follows the last entry. It is tar padding and must be zeros, and reaching the
    // end of the gzip member also checks its CRC32/ISIZE trailer. Anything after that member was
    // never decoded here and is refused; see `ConsumedSlice`.
    let mut tail = ar.into_inner();
    let mut drain = [0u8; 8192];
    loop {
        let n = tail.read(&mut drain).map_err(|err| {
            classify_source_ceiling(&err)
                .map(anyhow::Error::from)
                .unwrap_or_else(|| anyhow::Error::from(err).context("read gzip trailer"))
        })?;
        if n == 0 {
            break;
        }
        if drain[..n].iter().any(|&b| b != 0) {
            return Err(anyhow::Error::from(ReplayContractError::DataAfterArchive));
        }
    }
    if consumed.get() != snapshot.len() {
        return Err(anyhow::Error::from(
            ReplayContractError::DataAfterGzipStream,
        ));
    }

    let manifest_json = manifest_data.context("manifest.json missing in bundle")?;
    let manifest: ReplayManifest =
        serde_json::from_slice(&manifest_json).context("parse manifest.json")?;
    let entries = seen.into_iter().collect();
    Ok(ReadBundle { manifest, entries })
}
