use crate::mutators::bitflip::BitFlip;
use crate::mutators::inject::InjectFile;
use crate::mutators::truncate::Truncate;
use crate::mutators::Mutator;
use crate::report::{AttackResult, AttackStatus};
use anyhow::{Context, Result};
use assay_evidence::crypto::id::{compute_content_hash, compute_run_root};
use assay_evidence::types::EvidenceEvent;
use assay_evidence::{verify_bundle, BundleWriter};
use chrono::{TimeZone, Utc};
use sha2::{Digest, Sha256};
use std::io::{Cursor, Read};
use std::time::Instant;

/// Result from the reference (non-streaming) verifier.
#[derive(Debug)]
pub struct ReferenceResult {
    pub valid: bool,
    pub event_count: usize,
    pub run_root: String,
    pub error: Option<String>,
}

/// Independent reference verifier that does NOT use the production verify_bundle path.
///
/// Reads entire bundle into memory, decompresses gzip → tar, extracts
/// manifest.json + events.ndjson, parses with standard serde_json (no streaming),
/// and recomputes all hashes independently.
pub fn reference_verify(bundle_data: &[u8]) -> ReferenceResult {
    match reference_verify_inner(bundle_data) {
        Ok(r) => r,
        Err(e) => ReferenceResult {
            valid: false,
            event_count: 0,
            run_root: String::new(),
            error: Some(e.to_string()),
        },
    }
}

fn reference_verify_inner(bundle_data: &[u8]) -> Result<ReferenceResult> {
    // 1. Decompress gzip
    let decoder = flate2::read::GzDecoder::new(Cursor::new(bundle_data));
    let mut archive = tar::Archive::new(decoder);

    let mut manifest_bytes: Option<Vec<u8>> = None;
    let mut events_bytes: Option<Vec<u8>> = None;

    for entry in archive.entries().context("reading tar entries")? {
        let mut entry = entry.context("reading tar entry")?;
        let path = entry.path()?.to_string_lossy().to_string();

        let mut content = Vec::new();
        entry
            .read_to_end(&mut content)
            .context("reading entry content")?;

        match path.as_str() {
            "manifest.json" => manifest_bytes = Some(content),
            "events.ndjson" => events_bytes = Some(content),
            _ => {
                return Ok(ReferenceResult {
                    valid: false,
                    event_count: 0,
                    run_root: String::new(),
                    error: Some(format!("unexpected file: {}", path)),
                });
            }
        }
    }

    let manifest_bytes = manifest_bytes.context("missing manifest.json")?;
    let events_bytes = events_bytes.context("missing events.ndjson")?;

    // 2. Parse manifest
    let manifest: serde_json::Value =
        serde_json::from_slice(&manifest_bytes).context("parsing manifest")?;

    let declared_event_count = manifest
        .get("event_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let declared_run_root = manifest
        .get("run_root")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // 3. Verify events.ndjson hash
    let events_hash = format!("sha256:{}", hex::encode(Sha256::digest(&events_bytes)));
    let declared_events_hash = manifest
        .get("files")
        .and_then(|f| f.get("events.ndjson"))
        .and_then(|f| f.get("sha256"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if events_hash != declared_events_hash {
        return Ok(ReferenceResult {
            valid: false,
            event_count: 0,
            run_root: String::new(),
            error: Some(format!(
                "events hash mismatch: computed={}, declared={}",
                events_hash, declared_events_hash
            )),
        });
    }

    // 4. Parse events (non-streaming — all at once)
    let events_str = std::str::from_utf8(&events_bytes).context("events not valid UTF-8")?;
    let mut events: Vec<EvidenceEvent> = Vec::new();
    for line in events_str.lines() {
        if line.is_empty() {
            continue;
        }
        let event: EvidenceEvent = serde_json::from_str(line).context("parsing event")?;
        events.push(event);
    }

    // 5. Recompute content hashes and run_root
    let mut content_hashes = Vec::new();
    for event in &events {
        let computed = compute_content_hash(event).context("computing content hash")?;
        let claimed = event.content_hash.as_deref().unwrap_or("").to_string();

        if computed != claimed {
            return Ok(ReferenceResult {
                valid: false,
                event_count: events.len(),
                run_root: String::new(),
                error: Some(format!(
                    "content hash mismatch at seq {}: computed={}, claimed={}",
                    event.seq, computed, claimed
                )),
            });
        }
        content_hashes.push(computed);
    }

    let computed_run_root = compute_run_root(&content_hashes);

    // 6. Check all invariants
    if events.len() != declared_event_count {
        return Ok(ReferenceResult {
            valid: false,
            event_count: events.len(),
            run_root: computed_run_root,
            error: Some(format!(
                "event count mismatch: actual={}, declared={}",
                events.len(),
                declared_event_count
            )),
        });
    }

    if computed_run_root != declared_run_root {
        let error_msg = format!(
            "run root mismatch: computed={}, declared={}",
            computed_run_root, declared_run_root
        );
        return Ok(ReferenceResult {
            valid: false,
            event_count: events.len(),
            run_root: computed_run_root,
            error: Some(error_msg),
        });
    }

    Ok(ReferenceResult {
        valid: true,
        event_count: events.len(),
        run_root: computed_run_root,
        error: None,
    })
}

/// Run differential parity checks: apply mutations, compare production vs reference verifier.
///
/// For each mutation:
/// 1. Apply mutation to a valid bundle
/// 2. Run production `verify_bundle()` → result A
/// 3. Run `reference_verify()` → result B
/// 4. If A and B disagree → `AttackStatus::Failed` (SOTA parity violation)
/// 5. If both agree → `AttackStatus::Passed`
pub fn check_differential_parity(_seed: u64) -> Result<Vec<AttackResult>> {
    let valid_bundle = create_test_bundle()?;
    let mut results = Vec::new();

    // Define mutations to test
    let mutations: Vec<(&str, Box<dyn Mutator>)> = vec![
        (
            "differential.parity.bitflip",
            Box::new(BitFlip { count: 5 }),
        ),
        (
            "differential.parity.truncate",
            Box::new(Truncate {
                at: valid_bundle.len() / 2,
            }),
        ),
        (
            "differential.parity.inject",
            Box::new(InjectFile {
                name: "extra.txt".into(),
                content: b"injected".to_vec(),
            }),
        ),
    ];

    // Also test the unmodified bundle
    {
        let start = Instant::now();
        let production_ok = verify_bundle(Cursor::new(&valid_bundle)).is_ok();
        let reference = reference_verify(&valid_bundle);
        let duration = start.elapsed().as_millis() as u64;

        let agree = production_ok == reference.valid;
        results.push(AttackResult {
            name: "differential.parity.identity".into(),
            status: if agree {
                AttackStatus::Passed
            } else {
                AttackStatus::Failed
            },
            error_class: None,
            error_code: None,
            message: if agree {
                None
            } else {
                Some(format!(
                    "SOTA parity violation: production={}, reference={}",
                    production_ok, reference.valid
                ))
            },
            duration_ms: duration,
        });
    }

    // Test each mutation
    for (name, mutator) in mutations {
        let start = Instant::now();

        let mutated = match mutator.mutate(&valid_bundle) {
            Ok(m) => m,
            Err(e) => {
                let duration = start.elapsed().as_millis() as u64;
                results.push(AttackResult {
                    name: name.into(),
                    status: AttackStatus::Error,
                    error_class: None,
                    error_code: None,
                    message: Some(format!("mutation failed: {}", e)),
                    duration_ms: duration,
                });
                continue;
            }
        };

        let production_ok = verify_bundle(Cursor::new(&mutated)).is_ok();
        let reference = reference_verify(&mutated);
        let duration = start.elapsed().as_millis() as u64;

        let agree = production_ok == reference.valid;
        results.push(AttackResult {
            name: name.into(),
            status: if agree {
                AttackStatus::Passed
            } else {
                AttackStatus::Failed
            },
            error_class: None,
            error_code: None,
            message: if agree {
                None
            } else {
                Some(format!(
                    "SOTA parity violation: production={}, reference={}",
                    production_ok, reference.valid
                ))
            },
            duration_ms: duration,
        });
    }

    Ok(results)
}

fn create_test_bundle() -> Result<Vec<u8>> {
    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);
    for seq in 0..3u64 {
        let mut event = EvidenceEvent::new(
            "assay.test",
            "urn:test",
            "diffrun",
            seq,
            serde_json::json!({"seq": seq}),
        );
        event.time = Utc.timestamp_opt(1700000000 + seq as i64, 0).unwrap();
        writer.add_event(event);
    }
    writer.finish()?;
    Ok(buffer)
}
