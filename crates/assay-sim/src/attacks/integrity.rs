use crate::mutators::inject::InjectFile;
use crate::mutators::Mutator;
use crate::report::SimReport;
use anyhow::Result;
use assay_evidence::types::EvidenceEvent;
use assay_evidence::{verify_bundle, BundleWriter, VerifyError};
use chrono::{TimeZone, Utc};
use rand::Rng;
use rand::SeedableRng;
use std::io::Cursor;

pub fn check_integrity_attacks(report: &mut SimReport) -> Result<()> {
    let valid_bundle = create_test_bundle()?;

    // 1. BitFlip (Harder)
    run_attack(report, "integrity.bitflip", || {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let mut corrupted = valid_bundle.clone();
        for _ in 0..10 {
            let idx = rng.gen_range(0..corrupted.len());
            corrupted[idx] ^= 1 << rng.gen_range(0..8);
        }
        Ok(corrupted)
    })?;

    // 2. Truncate
    run_attack(report, "integrity.truncate", || {
        Ok(valid_bundle[..valid_bundle.len() / 2].to_vec())
    })?;

    // 3. Inject File
    run_attack(report, "integrity.inject_file", || {
        let injector = InjectFile {
            name: "malicious.sh".into(),
            content: b"echo 'bad'".to_vec(),
        };
        injector.mutate(&valid_bundle)
    })?;

    // 4. Zip Bomb
    run_attack(report, "security.zip_bomb", || {
        create_zip_bomb(1100 * 1024 * 1024)
    })?;

    // 5. [SOTA 2026] Tar Duplicate Entry
    run_attack(report, "integrity.tar_duplicate", || {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        {
            let mut builder = tar::Builder::new(&mut encoder);
            let manifest = serde_json::json!({
                "schema_version": 1, "run_id": "test", "event_count": 1, "run_root": "sha256:...",
                "files": { "events.ndjson": { "sha256": "..." } }
            });
            let manifest_bytes = serde_json::to_vec(&manifest)?;
            let mut header = tar::Header::new_gnu();
            header.set_path("manifest.json")?;
            header.set_size(manifest_bytes.len() as u64);
            header.set_cksum();
            builder.append(&header, manifest_bytes.as_slice())?;

            let event = create_event(0);
            let event_bytes = serde_json::to_vec(&event)?;
            for _ in 0..2 {
                let mut header = tar::Header::new_gnu();
                header.set_path("events.ndjson")?;
                header.set_size(event_bytes.len() as u64);
                header.set_cksum();
                builder.append(&header, event_bytes.as_slice())?;
            }
            builder.finish()?;
        }
        Ok(encoder.finish()?)
    })?;

    // 6. [SOTA 2026] NDJSON Nasties: BOM
    run_attack(report, "integrity.ndjson_bom", || {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        {
            let mut builder = tar::Builder::new(&mut encoder);
            let manifest = serde_json::json!({
                "schema_version": 1, "run_id": "test", "event_count": 1, "run_root": "sha256:...",
                "files": { "events.ndjson": { "sha256": "..." } }
            });
            let manifest_bytes = serde_json::to_vec(&manifest)?;
            let mut header = tar::Header::new_gnu();
            header.set_path("manifest.json")?;
            header.set_size(manifest_bytes.len() as u64);
            header.set_cksum();
            builder.append(&header, manifest_bytes.as_slice())?;

            let mut content = vec![0xEF, 0xBB, 0xBF];
            content.extend_from_slice(&serde_json::to_vec(&create_event(0))?);
            let mut header = tar::Header::new_gnu();
            header.set_path("events.ndjson")?;
            header.set_size(content.len() as u64);
            header.set_cksum();
            builder.append(&header, content.as_slice())?;
            builder.finish()?;
        }
        Ok(encoder.finish()?)
    })?;

    // 7. [SOTA 2026] NDJSON Nasties: CRLF
    run_attack(report, "integrity.ndjson_crlf", || {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        {
            let mut builder = tar::Builder::new(&mut encoder);
            // Append with \r\n
            let mut content = serde_json::to_vec(&create_event(0))?;
            content.extend_from_slice(b"\r\n");

            let mut header = tar::Header::new_gnu();
            header.set_path("events.ndjson")?;
            header.set_size(content.len() as u64);
            header.set_cksum();
            builder.append(&header, content.as_slice())?;
            builder.finish()?;
        }
        Ok(encoder.finish()?)
    })?;

    // 8. [SOTA 2026] Bundle Size Limit
    run_attack(report, "limits.bundle_size", || {
        // Create a bundle that is exactly 1 byte over the default limit (100MB) or just use a small limit.
        // For simulation, we can just return a large buffer.
        Ok(vec![0u8; 100 * 1024 * 1024 + 1])
    })?;

    Ok(())
}

fn create_test_bundle() -> Result<Vec<u8>> {
    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);
    writer.add_event(create_event(0));
    writer.finish()?;
    Ok(buffer)
}

fn create_event(seq: u64) -> EvidenceEvent {
    let mut event = EvidenceEvent::new("assay.test", "urn:test", "run", seq, serde_json::json!({}));
    event.time = Utc.timestamp_opt(1700000000, 0).unwrap();
    event
}

fn run_attack<F>(report: &mut SimReport, name: &str, mutator: F) -> Result<()>
where
    F: FnOnce() -> Result<Vec<u8>>,
{
    let data = mutator()?;
    let start = std::time::Instant::now();
    let res = verify_bundle(Cursor::new(data));
    let duration = start.elapsed().as_millis() as u64;

    match res {
        Ok(_) => report.add_attack(name, Err(anyhow::anyhow!("Attack Bypassed")), duration),
        Err(e) => {
            if let Some(ve) = e.downcast_ref::<VerifyError>() {
                report.add_attack(name, Ok((ve.class(), ve.code)), duration);
            } else {
                report.add_attack(
                    name,
                    Err(anyhow::anyhow!("Unexpected error: {}", e)),
                    duration,
                );
            }
        }
    }
    Ok(())
}

fn create_zip_bomb(target_uncompressed: u64) -> Result<Vec<u8>> {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    let mut buf = Vec::new();
    let mut encoder = GzEncoder::new(&mut buf, Compression::best());
    let chunk = vec![0u8; 1024 * 1024];
    let mut remaining = target_uncompressed;
    while remaining > 0 {
        let to_write = remaining.min(chunk.len() as u64);
        encoder.write_all(&chunk[..to_write as usize])?;
        remaining -= to_write;
    }
    encoder.finish()?;
    Ok(buf)
}
