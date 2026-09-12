use anyhow::{ensure, Result};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use sha2::{Digest, Sha256};
use std::io::{Cursor, Read, Write};

/// Writer-produced bundle with a second `manifest.json` member appended.
pub fn duplicate_manifest(valid_bundle: &[u8]) -> Result<Vec<u8>> {
    let tar = gunzip(valid_bundle)?;
    let members = split_tar_members(&tar)?;
    let manifest = *members
        .iter()
        .find(|member| tar_member_name(member) == "manifest.json")
        .ok_or_else(|| anyhow::anyhow!("valid bundle missing manifest.json"))?;
    let mut out = Vec::new();
    for member in members {
        out.extend_from_slice(member);
    }
    out.extend_from_slice(manifest);
    out.extend_from_slice(&[0u8; 1024]);
    gzip(&out)
}

/// Writer-produced bundle whose events member starts with a UTF-8 BOM.
pub fn ndjson_bom(valid_bundle: &[u8]) -> Result<Vec<u8>> {
    let mut members = unpack_members(valid_bundle)?;
    let events = member_bytes(&members, "events.ndjson")?.to_vec();
    let mut bom_events = vec![0xEF, 0xBB, 0xBF];
    bom_events.extend_from_slice(&events);
    reseal_events(&mut members, bom_events)?;
    pack_members(&members)
}

/// Writer-produced bundle whose events member uses CRLF line endings.
pub fn ndjson_crlf(valid_bundle: &[u8]) -> Result<Vec<u8>> {
    let mut members = unpack_members(valid_bundle)?;
    let events = member_bytes(&members, "events.ndjson")?.to_vec();
    reseal_events(&mut members, lf_to_crlf(&events))?;
    pack_members(&members)
}

/// Valid tar followed by enough zero padding that decoded size is `max_decode_bytes + 1`.
pub fn decode_bomb(valid_bundle: &[u8], limits: assay_evidence::VerifyLimits) -> Result<Vec<u8>> {
    let tar = gunzip(valid_bundle)?;
    ensure!(
        (tar.len() as u64) <= limits.max_decode_bytes,
        "valid bundle already exceeds max_decode_bytes"
    );
    let extra = limits
        .max_decode_bytes
        .saturating_add(1)
        .saturating_sub(tar.len() as u64);
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&tar)?;
    let chunk = vec![0u8; 1024 * 1024];
    let mut remaining = extra;
    while remaining > 0 {
        let n = remaining.min(chunk.len() as u64) as usize;
        encoder.write_all(&chunk[..n])?;
        remaining -= n as u64;
    }
    let out = encoder.finish()?;
    ensure!(
        (out.len() as u64) < limits.max_bundle_bytes,
        "decode bomb compressed {} bytes, which does not clear max_bundle_bytes {}",
        out.len(),
        limits.max_bundle_bytes
    );
    Ok(out)
}

/// Flip one bit in the gzip CRC32 trailer of a valid bundle.
pub fn flip_gzip_crc(valid_bundle: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        valid_bundle.len() >= 8,
        "bundle too small to have a gzip trailer"
    );
    let mut corrupted = valid_bundle.to_vec();
    let n = corrupted.len();
    corrupted[n - 8] ^= 1;
    Ok(corrupted)
}

fn gunzip(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    GzDecoder::new(Cursor::new(bytes)).read_to_end(&mut out)?;
    Ok(out)
}

fn gzip(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(bytes)?;
    Ok(encoder.finish()?)
}

fn split_tar_members(tar: &[u8]) -> Result<Vec<&[u8]>> {
    let mut members = Vec::new();
    let mut off = 0;
    while off + 512 <= tar.len() {
        if tar[off..off + 512].iter().all(|&b| b == 0) {
            break;
        }
        let len = tar_member_len(&tar[off..])?;
        ensure!(off + len <= tar.len(), "truncated tar member");
        members.push(&tar[off..off + len]);
        off += len;
    }
    ensure!(!members.is_empty(), "tar has no members");
    Ok(members)
}

fn tar_member_len(header_and_rest: &[u8]) -> Result<usize> {
    ensure!(header_and_rest.len() >= 512, "truncated tar header");
    let field = std::str::from_utf8(&header_and_rest[124..135])?.trim();
    let size = u64::from_str_radix(field.trim_matches(|c: char| c == ' ' || c == '\0'), 8)?;
    let padded = size.div_ceil(512) * 512;
    Ok(512 + padded as usize)
}

fn tar_member_name(member: &[u8]) -> String {
    let name: Vec<u8> = member[..100.min(member.len())]
        .iter()
        .copied()
        .take_while(|&b| b != 0)
        .collect();
    String::from_utf8_lossy(&name).into_owned()
}

fn unpack_members(bundle: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    let mut archive = tar::Archive::new(GzDecoder::new(Cursor::new(bundle)));
    let mut members = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().into_owned();
        let mut data = Vec::new();
        entry.read_to_end(&mut data)?;
        members.push((path, data));
    }
    Ok(members)
}

fn pack_members(members: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    {
        let mut builder = tar::Builder::new(&mut encoder);
        builder.mode(tar::HeaderMode::Deterministic);
        for (path, data) in members {
            let mut header = tar::Header::new_gnu();
            header.set_path(path)?;
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_uid(0);
            header.set_gid(0);
            header.set_mtime(0);
            header.set_cksum();
            builder.append(&header, data.as_slice())?;
        }
        builder.finish()?;
    }
    Ok(encoder.finish()?)
}

fn member_bytes<'a>(members: &'a [(String, Vec<u8>)], name: &str) -> Result<&'a [u8]> {
    members
        .iter()
        .find(|(path, _)| path == name)
        .map(|(_, data)| data.as_slice())
        .ok_or_else(|| anyhow::anyhow!("bundle missing {name}"))
}

fn reseal_events(members: &mut [(String, Vec<u8>)], events: Vec<u8>) -> Result<()> {
    let digest = format!("sha256:{}", hex::encode(Sha256::digest(&events)));
    let bytes = events.len() as u64;
    let manifest_pos = members
        .iter()
        .position(|(path, _)| path == "manifest.json")
        .ok_or_else(|| anyhow::anyhow!("bundle missing manifest.json"))?;
    let mut manifest: serde_json::Value = serde_json::from_slice(&members[manifest_pos].1)?;
    manifest["files"]["events.ndjson"]["sha256"] = serde_json::Value::String(digest);
    manifest["files"]["events.ndjson"]["bytes"] = serde_json::json!(bytes);
    members[manifest_pos].1 = serde_json::to_vec(&manifest)?;
    let events_pos = members
        .iter()
        .position(|(path, _)| path == "events.ndjson")
        .ok_or_else(|| anyhow::anyhow!("bundle missing events.ndjson"))?;
    members[events_pos].1 = events;
    Ok(())
}

fn lf_to_crlf(events: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(events.len() + 8);
    for (i, &b) in events.iter().enumerate() {
        if b == b'\n' && (i == 0 || events[i - 1] != b'\r') {
            out.push(b'\r');
        }
        out.push(b);
    }
    out
}
