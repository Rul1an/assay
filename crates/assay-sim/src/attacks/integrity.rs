use super::test_bundle::create_single_event_bundle;
use crate::mutators::inject::InjectFile;
use crate::mutators::Mutator;
use crate::report::SimReport;
use crate::suite::TimeBudget;
use anyhow::Result as AnyhowResult;
use assay_evidence::types::EvidenceEvent;
use assay_evidence::{verify_bundle_with_limits, VerifyError, VerifyLimits};
use chrono::{TimeZone, Utc};
use flate2::read::GzEncoder;
use flate2::Compression;
use rand::Rng;
use rand::SeedableRng;
use std::io::{self, Cursor, Read};

pub fn check_integrity_attacks(
    report: &mut SimReport,
    seed: u64,
    limits: VerifyLimits,
    budget: &TimeBudget,
) -> Result<(), IntegrityError> {
    let valid_bundle = create_single_event_bundle().map_err(IntegrityError::from)?;

    // 1. BitFlip (Harder)
    run_attack(report, "integrity.bitflip", limits, budget, || {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let mut corrupted = valid_bundle.clone();
        for _ in 0..10 {
            let idx = rng.gen_range(0..corrupted.len());
            corrupted[idx] ^= 1 << rng.gen_range(0..8);
        }
        Ok(corrupted)
    })?;

    // 2. Truncate
    run_attack(report, "integrity.truncate", limits, budget, || {
        Ok(valid_bundle[..valid_bundle.len() / 2].to_vec())
    })?;

    // 3. Inject File
    run_attack(report, "integrity.inject_file", limits, budget, || {
        let injector = InjectFile {
            name: "malicious.sh".into(),
            content: b"echo 'bad'".to_vec(),
        };
        injector.mutate(&valid_bundle)
    })?;

    // 4. Zip Bomb
    run_attack(report, "security.zip_bomb", limits, budget, || {
        create_zip_bomb(1100 * 1024 * 1024)
    })?;

    // 5. [SOTA 2026] Tar Duplicate Entry
    run_attack(report, "integrity.tar_duplicate", limits, budget, || {
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
    run_attack(report, "integrity.ndjson_bom", limits, budget, || {
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
    run_attack(report, "integrity.ndjson_crlf", limits, budget, || {
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

    // 8. limit_bundle_bytes (ADR-024): compressed size = limit + 1, streaming (no alloc)
    run_attack_reader(
        report,
        "integrity.limit_bundle_bytes",
        limits,
        budget,
        || {
            let n = limits.max_bundle_bytes.saturating_add(1);
            let src = io::repeat(0u8).take(n);
            Ok(GzEncoder::new(src, Compression::none()))
        },
    )?;

    Ok(())
}

fn create_event(seq: u64) -> EvidenceEvent {
    let mut event = EvidenceEvent::new("assay.test", "urn:test", "run", seq, serde_json::json!({}));
    event.time = Utc.timestamp_opt(1700000000, 0).unwrap();
    event
}

#[derive(Debug)]
pub enum IntegrityError {
    BudgetExceeded,
    Other(anyhow::Error),
}
impl From<anyhow::Error> for IntegrityError {
    fn from(e: anyhow::Error) -> Self {
        Self::Other(e)
    }
}

fn run_attack_reader<F, R>(
    report: &mut SimReport,
    name: &str,
    limits: VerifyLimits,
    budget: &TimeBudget,
    make_reader: F,
) -> Result<(), IntegrityError>
where
    F: FnOnce() -> AnyhowResult<R>,
    R: Read,
{
    if budget.exceeded() {
        return Err(IntegrityError::BudgetExceeded);
    }
    let reader = make_reader()?;
    let start = std::time::Instant::now();
    let res = verify_bundle_with_limits(reader, limits);
    if budget.exceeded() {
        return Err(IntegrityError::BudgetExceeded);
    }
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

fn run_attack<F>(
    report: &mut SimReport,
    name: &str,
    limits: VerifyLimits,
    budget: &TimeBudget,
    mutator: F,
) -> Result<(), IntegrityError>
where
    F: FnOnce() -> AnyhowResult<Vec<u8>>,
{
    run_attack_reader(report, name, limits, budget, || {
        let data = mutator()?;
        Ok(Cursor::new(data))
    })
}

fn create_zip_bomb(target_uncompressed: u64) -> AnyhowResult<Vec<u8>> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::AttackStatus;
    use crate::suite::TimeBudget;
    use assay_evidence::VerifyLimits;
    #[test]
    fn test_limit_bundle_bytes_blocked_with_limit_bundle_bytes() {
        // Use limit 100: gzip from 1001 zeros is ~1024 bytes, so LimitReader must trigger.
        let limits = VerifyLimits {
            max_bundle_bytes: 100,
            ..Default::default()
        };

        let mut report = SimReport::new("test", 0);
        let budget = TimeBudget::new(std::time::Duration::from_secs(60));

        check_integrity_attacks(&mut report, 0, limits, &budget).unwrap();

        let r = report
            .results
            .iter()
            .find(|r| r.name == "integrity.limit_bundle_bytes")
            .expect("limit_bundle_bytes result");
        assert_eq!(r.status, AttackStatus::Blocked);
        assert_eq!(
            r.error_code.as_deref(),
            Some("LimitBundleBytes"),
            "expected LimitBundleBytes, got {:?}",
            r.error_code
        );
    }
}
