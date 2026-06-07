//! Mutation-detection matrix for signed evidence bundles.
//!
//! Question this measures: which post-hoc mutation classes are detected, by which verifier check,
//! under a no-signing-key attacker model? Competitors say "signed" / "verifiable"; this publishes
//! the matrix.
//!
//! Threat model T: a party WITHOUT the signing key mutates a bundle after the fact. The bundle's
//! run anchor (`run_root`, a Merkle root) is bound by an external signature the attacker cannot
//! forge. Two layers:
//!   - internal verifier (`verify_bundle_with_limits`): catches blind tampering that breaks
//!     internal consistency, with a specific ErrorCode;
//!   - run anchor (`run_root`): catches a *consistent rewrite* (events + manifest recomputed) that
//!     the internal verifier alone would accept, because the content-addressed root changes and the
//!     external signature over the original root no longer matches.
//!
//! Honesty: tamper-EVIDENT, not tamper-proof. A host holding BOTH code-exec and the signing key is
//! out of scope (it can re-sign); that needs external anchoring such as a transparency log or TSA.
//!
//! Security gate (always-on in CI): no mutation may BYPASS detection (changed evidence content that
//! still verifies AND keeps the original anchor). The full matrix + JSON is emitted only when
//! `E3_OUT_DIR` is set.

use assay_evidence::types::EvidenceEvent;
use assay_evidence::{verify_bundle_with_limits, BundleWriter, VerifyError, VerifyLimits};
use assay_sim::mutators::bitflip::BitFlip;
use assay_sim::mutators::inject::InjectFile;
use assay_sim::mutators::truncate::Truncate;
use assay_sim::mutators::Mutator;
use chrono::{TimeZone, Utc};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde_json::json;
use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

// ----- bundle helpers ---------------------------------------------------------

fn payload_blob(seq: u64) -> String {
    // Deterministic, content-bearing so edits land inside real evidence.
    format!("path=/workspace/file-{seq:04}.txt;decision=allow;nonce={seq:08x}")
}

/// Build a deterministic bundle of `m` events. If `mutate_seq` is set, that event's payload is
/// altered (and the writer recomputes the manifest + run_root -> a *consistent* rewrite).
fn build_bundle(m: u64, mutate_seq: Option<u64>) -> Vec<u8> {
    let mut bundle = Vec::new();
    let mut writer = BundleWriter::new(&mut bundle);
    for seq in 0..m {
        let mut blob = payload_blob(seq);
        if Some(seq) == mutate_seq {
            blob.push_str(";TAMPERED");
        }
        let time = Utc
            .timestamp_opt(1_700_000_000_i64 + seq as i64, 0)
            .unwrap();
        let event = EvidenceEvent::new(
            "assay.tool.decision",
            "urn:assay:e3-matrix",
            "run-matrix".to_string(),
            seq,
            json!({ "tool": "fs.read", "args": { "detail": blob }, "decision": "allow" }),
        )
        .with_time(time);
        writer.add_event(event);
    }
    writer.finish().expect("bundle generation must succeed");
    bundle
}

fn run_root_of(bundle: &[u8]) -> String {
    verify_bundle_with_limits(Cursor::new(bundle), VerifyLimits::default())
        .expect("base bundle must verify")
        .computed_run_root
}

fn unpack(bundle: &[u8]) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    let mut archive = tar::Archive::new(GzDecoder::new(Cursor::new(bundle)));
    let mut manifest = None;
    let mut events = None;
    for entry in archive.entries()? {
        let mut e = entry?;
        let path = e.path()?.to_string_lossy().to_string();
        let mut buf = Vec::new();
        e.read_to_end(&mut buf)?;
        match path.as_str() {
            "manifest.json" => manifest = Some(buf),
            "events.ndjson" => events = Some(buf),
            _ => {}
        }
    }
    Ok((
        manifest.ok_or_else(|| anyhow::anyhow!("no manifest.json"))?,
        events.ok_or_else(|| anyhow::anyhow!("no events.ndjson"))?,
    ))
}

fn append(builder: &mut tar::Builder<&mut GzEncoder<Vec<u8>>>, name: &str, content: &[u8]) {
    let mut header = tar::Header::new_gnu();
    header.set_path(name).unwrap();
    header.set_size(content.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append(&header, content).unwrap();
}

/// Repack manifest.json (first) + events.ndjson (second) into a new tar.gz.
fn repack(manifest: &[u8], events: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    {
        let mut builder = tar::Builder::new(&mut encoder);
        append(&mut builder, "manifest.json", manifest);
        append(&mut builder, "events.ndjson", events);
        builder.finish().unwrap();
    }
    encoder.finish().unwrap()
}

// The standard tar writer refuses to emit `..` or absolute paths (a writer-side defense), so to
// exercise the verifier's path-safety checks we hand-roll a minimal USTAR archive that can carry an
// unsafe entry name.
fn write_octal(field: &mut [u8], val: u64, digits: usize) {
    let s = format!("{val:0digits$o}");
    let b = s.as_bytes();
    let n = b.len().min(digits);
    field[..n].copy_from_slice(&b[b.len() - n..]);
    if field.len() > digits {
        field[digits] = 0;
    }
}

fn raw_tar(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, content) in entries {
        let mut h = [0u8; 512];
        let nb = name.as_bytes();
        let nlen = nb.len().min(100);
        h[..nlen].copy_from_slice(&nb[..nlen]);
        write_octal(&mut h[100..108], 0o644, 7);
        write_octal(&mut h[108..116], 0, 7);
        write_octal(&mut h[116..124], 0, 7);
        write_octal(&mut h[124..136], content.len() as u64, 11);
        write_octal(&mut h[136..148], 0, 11);
        for b in &mut h[148..156] {
            *b = b' ';
        }
        h[156] = b'0'; // regular file
        h[257..262].copy_from_slice(b"ustar");
        h[263] = b'0';
        h[264] = b'0';
        let sum: u32 = h.iter().map(|&b| b as u32).sum();
        let cs = format!("{sum:06o}");
        h[148..154].copy_from_slice(cs.as_bytes());
        h[154] = 0;
        h[155] = b' ';
        out.extend_from_slice(&h);
        out.extend_from_slice(content);
        let pad = (512 - (content.len() % 512)) % 512;
        out.extend(std::iter::repeat_n(0u8, pad));
    }
    out.extend(std::iter::repeat_n(0u8, 1024));
    out
}

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut e = GzEncoder::new(Vec::new(), Compression::default());
    e.write_all(data).unwrap();
    e.finish().unwrap()
}

// ----- classification ---------------------------------------------------------

/// Protected surface = the event evidence (events.ndjson) and the run anchor (run_root, recomputed
/// from event content hashes). Outcomes are judged against THAT surface:
///   - Detected: the verifier rejected the mutant.
///   - NoOp: verified, and both manifest + events decompress byte-identical to the original.
///   - ManifestMetaOnly: verified, events identical, but a manifest metadata byte differs. This is a
///     documented limitation, not an evidence bypass: manifest metadata not referenced by a check
///     (e.g. a producer label) is not individually hash-bound, and a truncated tar read may skip the
///     gzip CRC trailer. The event evidence and run anchor remain protected.
///   - Bypass: verified, but the EVENT evidence changed — a real detection failure (must be zero).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Detected,
    NoOp,
    ManifestMetaOnly,
    Bypass,
}

#[derive(Default)]
struct Tally {
    detected: usize,
    noop: usize,
    manifest_meta_only: usize,
    bypass: usize,
    codes: BTreeMap<String, usize>,
}

impl Tally {
    fn record(&mut self, original_pair: &(Vec<u8>, Vec<u8>), mutant: &[u8]) -> Outcome {
        match verify_bundle_with_limits(Cursor::new(mutant), VerifyLimits::default()) {
            Err(e) => {
                let code = e
                    .downcast_ref::<VerifyError>()
                    .map(|ve| ve.code.to_string())
                    .unwrap_or_else(|| "RejectedNonVerifyError".to_string());
                *self.codes.entry(code).or_default() += 1;
                self.detected += 1;
                Outcome::Detected
            }
            Ok(_) => match unpack(mutant) {
                Ok((m2, e2)) => {
                    if e2 != original_pair.1 {
                        // The event evidence itself changed yet verified: a real failure.
                        self.bypass += 1;
                        Outcome::Bypass
                    } else if m2 != original_pair.0 {
                        self.manifest_meta_only += 1;
                        Outcome::ManifestMetaOnly
                    } else {
                        self.noop += 1;
                        Outcome::NoOp
                    }
                }
                // Verified but no longer unpackable: treat conservatively as a failure.
                Err(_) => {
                    self.bypass += 1;
                    Outcome::Bypass
                }
            },
        }
    }

    fn dominant_code(&self) -> Option<String> {
        self.codes
            .iter()
            .max_by_key(|(_, n)| **n)
            .map(|(c, _)| c.clone())
    }

    fn to_json(&self, class: &str) -> serde_json::Value {
        json!({
            "class": class,
            "layer": "internal-verifier",
            "detected": self.detected,
            "noop": self.noop,
            "manifest_meta_only": self.manifest_meta_only,
            "bypass": self.bypass,
            "dominant_code": self.dominant_code(),
            "codes": self.codes,
        })
    }
}

// ----- the matrix -------------------------------------------------------------

struct SweepCfg {
    base_events: u64,
    bitflip_counts: Vec<usize>,
    bitflip_seeds: u64,
    truncate_fracs: Vec<f64>,
    edit_seeds: u64,
}

fn ci_cfg() -> SweepCfg {
    SweepCfg {
        base_events: 24,
        bitflip_counts: vec![1, 2, 8],
        bitflip_seeds: 12,
        truncate_fracs: vec![0.01, 0.10, 0.50, 0.90, 0.99],
        edit_seeds: 6,
    }
}

fn full_cfg() -> SweepCfg {
    SweepCfg {
        base_events: 64,
        bitflip_counts: vec![1, 2, 4, 8, 16, 32, 64],
        bitflip_seeds: 64,
        truncate_fracs: vec![0.001, 0.01, 0.05, 0.10, 0.25, 0.50, 0.75, 0.90, 0.99],
        edit_seeds: 32,
    }
}

struct MatrixResult {
    classes: Vec<serde_json::Value>,
    total_bypass: usize,
    total_manifest_meta_only: usize,
    anchored_root: String,
    rewrite_root: String,
}

fn run_matrix(cfg: &SweepCfg) -> MatrixResult {
    let base = build_bundle(cfg.base_events, None);
    let original_pair = unpack(&base).expect("unpack base");
    let mut classes = Vec::new();
    let mut total_bypass = 0usize;

    // 1. gzip_bitflip: flip raw .tar.gz bytes.
    {
        let mut t = Tally::default();
        for &count in &cfg.bitflip_counts {
            for seed in 0..cfg.bitflip_seeds {
                let m = BitFlip {
                    count,
                    seed: Some(seed),
                }
                .mutate(&base)
                .unwrap();
                t.record(&original_pair, &m);
            }
        }
        total_bypass += t.bypass;
        classes.push(t.to_json("gzip_bitflip"));
    }

    // 2. truncate: cut the bundle at a fraction of its length.
    {
        let mut t = Tally::default();
        for &frac in &cfg.truncate_fracs {
            let at = ((base.len() as f64) * frac) as usize;
            let m = Truncate { at }.mutate(&base).unwrap();
            t.record(&original_pair, &m);
        }
        total_bypass += t.bypass;
        classes.push(t.to_json("truncate"));
    }

    // 3. inject_file: add a disallowed extra file.
    {
        let mut t = Tally::default();
        let m = InjectFile {
            name: "malicious.sh".into(),
            content: b"echo bad".to_vec(),
        }
        .mutate(&base)
        .unwrap();
        t.record(&original_pair, &m);
        total_bypass += t.bypass;
        classes.push(t.to_json("inject_file"));
    }

    // 4. inject_path_traversal: a hand-rolled tar entry with a `..` path.
    {
        let mut t = Tally::default();
        let (manifest, events) = original_pair.clone();
        let m = gzip(&raw_tar(&[
            ("manifest.json", &manifest),
            ("events.ndjson", &events),
            ("../evil.sh", b"echo bad"),
        ]));
        t.record(&original_pair, &m);
        total_bypass += t.bypass;
        classes.push(t.to_json("inject_path_traversal"));
    }

    // 4b. inject_absolute_path: a hand-rolled tar entry with an absolute path.
    {
        let mut t = Tally::default();
        let (manifest, events) = original_pair.clone();
        let m = gzip(&raw_tar(&[
            ("manifest.json", &manifest),
            ("events.ndjson", &events),
            ("/etc/evil.sh", b"echo bad"),
        ]));
        t.record(&original_pair, &m);
        total_bypass += t.bypass;
        classes.push(t.to_json("inject_absolute_path"));
    }

    // 5. event_byte_edit: flip a byte inside events.ndjson, keep the original manifest.
    {
        let mut t = Tally::default();
        let (manifest, events) = original_pair.clone();
        for s in 0..cfg.edit_seeds {
            let mut ev = events.clone();
            if ev.len() > 8 {
                let idx = 4 + (s as usize * 7) % (ev.len() - 8);
                ev[idx] ^= 0x20; // toggle a bit in an ASCII byte
            }
            let m = repack(&manifest, &ev);
            t.record(&original_pair, &m);
        }
        total_bypass += t.bypass;
        classes.push(t.to_json("event_byte_edit"));
    }

    // 6. event_drop: remove the last event line, keep the original manifest.
    {
        let mut t = Tally::default();
        let (manifest, events) = original_pair.clone();
        let text = String::from_utf8_lossy(&events);
        let mut lines: Vec<&str> = text.lines().collect();
        lines.pop();
        let dropped = format!("{}\n", lines.join("\n"));
        let m = repack(&manifest, dropped.as_bytes());
        t.record(&original_pair, &m);
        total_bypass += t.bypass;
        classes.push(t.to_json("event_drop"));
    }

    // 7. event_reorder: swap the first two event lines, keep the original manifest.
    {
        let mut t = Tally::default();
        let (manifest, events) = original_pair.clone();
        let text = String::from_utf8_lossy(&events);
        let mut lines: Vec<&str> = text.lines().collect();
        if lines.len() >= 2 {
            lines.swap(0, 1);
        }
        let reordered = format!("{}\n", lines.join("\n"));
        let m = repack(&manifest, reordered.as_bytes());
        t.record(&original_pair, &m);
        total_bypass += t.bypass;
        classes.push(t.to_json("event_reorder"));
    }

    // 8. ndjson_bom: prepend a UTF-8 BOM to events.ndjson.
    {
        let mut t = Tally::default();
        let (manifest, events) = original_pair.clone();
        let mut bom = vec![0xEF, 0xBB, 0xBF];
        bom.extend_from_slice(&events);
        let m = repack(&manifest, &bom);
        t.record(&original_pair, &m);
        total_bypass += t.bypass;
        classes.push(t.to_json("ndjson_bom"));
    }

    // 9. ndjson_crlf: append a CRLF terminator.
    {
        let mut t = Tally::default();
        let (manifest, events) = original_pair.clone();
        let mut crlf = events.clone();
        crlf.extend_from_slice(b"\r\n");
        let m = repack(&manifest, &crlf);
        t.record(&original_pair, &m);
        total_bypass += t.bypass;
        classes.push(t.to_json("ndjson_crlf"));
    }

    // 10. tar_duplicate: events.ndjson appears twice.
    {
        let mut t = Tally::default();
        let (manifest, events) = original_pair.clone();
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        {
            let mut builder = tar::Builder::new(&mut encoder);
            append(&mut builder, "manifest.json", &manifest);
            append(&mut builder, "events.ndjson", &events);
            append(&mut builder, "events.ndjson", &events);
            builder.finish().unwrap();
        }
        let m = encoder.finish().unwrap();
        t.record(&original_pair, &m);
        total_bypass += t.bypass;
        classes.push(t.to_json("tar_duplicate"));
    }

    // 11. consistent_rewrite: rewrite an event AND recompute the manifest/run_root. The internal
    //     verifier passes (expected); the content-addressed run anchor changes, so an external
    //     signature over the original root rejects it.
    let anchored_root = run_root_of(&base);
    let rewrite = build_bundle(cfg.base_events, Some(cfg.base_events / 2));
    let rewrite_verified =
        verify_bundle_with_limits(Cursor::new(&rewrite), VerifyLimits::default())
            .expect("consistent rewrite must pass the internal verifier by construction");
    let rewrite_root = rewrite_verified.computed_run_root;
    classes.push(json!({
        "class": "consistent_rewrite",
        "layer": "run-anchor",
        "internal_verifier_passed": true,
        "detected_by_anchor": rewrite_root != anchored_root,
        "anchored_run_root": anchored_root,
        "mutated_run_root": rewrite_root,
        "note": "internal verifier alone is insufficient; the run anchor / external signature is what detects this",
    }));

    let total_manifest_meta_only = classes
        .iter()
        .filter_map(|c| c["manifest_meta_only"].as_u64())
        .sum::<u64>() as usize;

    MatrixResult {
        classes,
        total_bypass,
        total_manifest_meta_only,
        anchored_root,
        rewrite_root,
    }
}

#[test]
fn e3_mutation_detection_matrix() {
    let cfg = ci_cfg();
    let result = run_matrix(&cfg);

    if result.total_bypass > 0 {
        eprintln!(
            "BYPASS DEBUG:\n{}",
            serde_json::to_string_pretty(&result.classes).unwrap()
        );
    }

    // SECURITY GATE: no blind-tamper mutation may bypass detection.
    assert_eq!(
        result.total_bypass, 0,
        "SECURITY: {} mutation(s) changed evidence content yet passed the internal verifier",
        result.total_bypass
    );

    // The run anchor must catch the consistent rewrite the internal verifier accepts.
    assert!(
        result.anchored_run_root_differs(),
        "run anchor must change under a consistent rewrite"
    );

    // Sanity: every internal-verifier class actually detected at least one mutation (the sweep ran).
    for class in &result.classes {
        if class["layer"] == "internal-verifier" {
            let detected = class["detected"].as_u64().unwrap_or(0);
            let name = class["class"].as_str().unwrap_or("?");
            assert!(
                detected >= 1,
                "class '{name}' produced no detections — sweep may not have run"
            );
        }
    }

    // Full matrix + JSON only when requested.
    if let Ok(dir) = std::env::var("E3_OUT_DIR") {
        if !dir.is_empty() {
            let full = run_matrix(&full_cfg());
            assert_eq!(full.total_bypass, 0, "SECURITY: bypass in full sweep");
            let matrix = json!({
                "schema": "assay.experiment.evidence_mutation_matrix.v0",
                "threat_model": "post-hoc mutation by a party without the signing key; the run anchor (run_root) is bound by an external signature the attacker cannot forge",
                "non_goal": "tamper-proof; a host holding code-exec AND the signing key is out of scope and needs external anchoring (transparency log / TSA)",
                "base_events": full_cfg().base_events,
                "classes": full.classes,
                "gate": {
                    "total_bypass": full.total_bypass,
                    "total_manifest_meta_only": full.total_manifest_meta_only,
                },
                "documented_limitation": "manifest metadata not referenced by a verifier check is not individually hash-bound (manifest_meta_only); the event evidence (events.ndjson) and the run anchor (run_root) are always hash-checked",
            });
            std::fs::create_dir_all(&dir).expect("create out dir");
            std::fs::write(
                format!("{dir}/matrix.json"),
                serde_json::to_string_pretty(&matrix).unwrap(),
            )
            .expect("write matrix.json");
        }
    }
}

impl MatrixResult {
    fn anchored_run_root_differs(&self) -> bool {
        self.rewrite_root != self.anchored_root
    }
}
