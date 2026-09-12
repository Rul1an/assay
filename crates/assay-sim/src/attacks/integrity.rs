use super::integrity_payloads;
use super::test_bundle::create_single_event_bundle;
use crate::mutators::inject::InjectFile;
use crate::mutators::Mutator;
use crate::report::SimReport;
use crate::suite::TimeBudget;
use anyhow::Result as AnyhowResult;
use assay_evidence::{verify_bundle_with_limits, VerifyError, VerifyLimits};
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

    // 1. BitFlip (seeded; refusing rule depends on where the flips land)
    run_attack(report, "integrity.bitflip", limits, budget, || {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let mut corrupted = valid_bundle.clone();
        for _ in 0..10 {
            let idx = rng.gen_range(0..corrupted.len());
            corrupted[idx] ^= 1 << rng.gen_range(0..8);
        }
        Ok(corrupted)
    })?;

    // 1b. CRC32 trailer bitflip: only the gzip drain can refuse this bundle.
    run_attack(report, "integrity.bitflip_crc", limits, budget, || {
        integrity_payloads::flip_gzip_crc(&valid_bundle)
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

    // 4. Zip bomb: decoded-bytes ceiling, sized from the limits this run uses.
    run_attack(report, "security.zip_bomb", limits, budget, || {
        integrity_payloads::decode_bomb(&valid_bundle, limits)
    })?;

    // 5. Duplicate manifest.json on a writer-produced bundle.
    run_attack(report, "integrity.tar_duplicate", limits, budget, || {
        integrity_payloads::duplicate_manifest(&valid_bundle)
    })?;

    // 6. NDJSON BOM on a writer-produced bundle (reaches the event parser).
    run_attack(report, "integrity.ndjson_bom", limits, budget, || {
        integrity_payloads::ndjson_bom(&valid_bundle)
    })?;

    // 7. CRLF is tolerated: a valid bundle with CRLF events must still verify.
    run_invariant(report, "integrity.ndjson_crlf", limits, budget, || {
        integrity_payloads::ndjson_crlf(&valid_bundle)
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

#[derive(Debug)]
#[non_exhaustive]
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
    let duration = start.elapsed().as_millis() as u64;

    match res {
        Ok(_) => report.add_attack(name, Err(anyhow::anyhow!("Attack Bypassed")), duration),
        Err(e) => {
            if let Some(ve) = e.downcast_ref::<VerifyError>() {
                report.add_attack(
                    name,
                    Ok((ve.class(), ve.code, ve.message.clone())),
                    duration,
                );
            } else {
                report.add_attack(
                    name,
                    Err(anyhow::anyhow!("Unexpected error: {}", e)),
                    duration,
                );
            }
        }
    }

    if budget.exceeded() {
        return Err(IntegrityError::BudgetExceeded);
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

fn run_invariant<F>(
    report: &mut SimReport,
    name: &str,
    limits: VerifyLimits,
    budget: &TimeBudget,
    mutator: F,
) -> Result<(), IntegrityError>
where
    F: FnOnce() -> AnyhowResult<Vec<u8>>,
{
    if budget.exceeded() {
        return Err(IntegrityError::BudgetExceeded);
    }
    let data = mutator()?;
    let start = std::time::Instant::now();
    let res = verify_bundle_with_limits(Cursor::new(data), limits);
    let duration = start.elapsed().as_millis() as u64;
    match res {
        Ok(_) => report.add_check(name, Ok(()), duration),
        Err(e) => report.add_check(
            name,
            Err(anyhow::anyhow!("CRLF bundle was refused: {e}")),
            duration,
        ),
    }
    if budget.exceeded() {
        return Err(IntegrityError::BudgetExceeded);
    }
    Ok(())
}

/// Run only the limit_bundle_bytes attack. Used by tests to avoid the decode bomb.
#[cfg(test)]
fn run_limit_bundle_bytes_only(
    report: &mut SimReport,
    limits: VerifyLimits,
    budget: &TimeBudget,
) -> Result<(), IntegrityError> {
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
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::AttackStatus;
    use crate::suite::TimeBudget;
    use assay_evidence::VerifyLimits;
    use std::io::Cursor;

    fn verify_payload(
        bytes: &[u8],
        limits: VerifyLimits,
    ) -> (AttackStatus, Option<String>, String) {
        match verify_bundle_with_limits(Cursor::new(bytes), limits) {
            Ok(_) => (AttackStatus::Passed, None, String::new()),
            Err(e) => {
                let ve = e
                    .downcast_ref::<VerifyError>()
                    .expect("expected a typed VerifyError");
                (
                    AttackStatus::Blocked,
                    Some(format!("{:?}", ve.code)),
                    ve.message.clone(),
                )
            }
        }
    }

    #[test]
    fn test_limit_bundle_bytes_blocked_with_limit_bundle_bytes() {
        // Use limit 100: gzip from 101 zeros is ~1024 bytes compressed, so LimitReader must trigger.
        // Runs in isolation so the decode-bomb construction in check_integrity_attacks is not
        // part of this pin.
        let limits = VerifyLimits {
            max_bundle_bytes: 100,
            ..Default::default()
        };

        let mut report = SimReport::new("test", 0);
        let budget = TimeBudget::new(std::time::Duration::from_secs(60));

        run_limit_bundle_bytes_only(&mut report, limits, &budget).unwrap();

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

    #[test]
    fn tar_duplicate_is_refused_as_duplicate_file() {
        let bundle =
            integrity_payloads::duplicate_manifest(&create_single_event_bundle().unwrap()).unwrap();
        let (status, code, _) = verify_payload(&bundle, VerifyLimits::default());
        assert_eq!(status, AttackStatus::Blocked);
        assert_eq!(code.as_deref(), Some("ContractDuplicateFile"));
    }

    #[test]
    fn ndjson_bom_names_the_bom_rule() {
        let bundle =
            integrity_payloads::ndjson_bom(&create_single_event_bundle().unwrap()).unwrap();
        let (status, code, message) = verify_payload(&bundle, VerifyLimits::default());
        assert_eq!(status, AttackStatus::Blocked);
        assert_eq!(code.as_deref(), Some("ContractInvalidJson"));
        assert!(
            message.contains("BOM not allowed"),
            "reached the event parser but not the BOM rule: {message}"
        );
    }

    #[test]
    fn ndjson_crlf_on_a_valid_bundle_still_verifies() {
        let bundle =
            integrity_payloads::ndjson_crlf(&create_single_event_bundle().unwrap()).unwrap();
        let (status, code, message) = verify_payload(&bundle, VerifyLimits::default());
        assert_eq!(
            status,
            AttackStatus::Passed,
            "CRLF is tolerated; refused as {code:?}: {message}"
        );
    }

    #[test]
    fn bitflip_crc_is_refused_as_integrity_gzip() {
        let bundle =
            integrity_payloads::flip_gzip_crc(&create_single_event_bundle().unwrap()).unwrap();
        let (status, code, message) = verify_payload(&bundle, VerifyLimits::default());
        assert_eq!(status, AttackStatus::Blocked);
        assert_eq!(code.as_deref(), Some("IntegrityGzip"));
        assert!(
            message.contains("Gzip trailer"),
            "CRC flip must be refused by the trailer drain, got {message}"
        );
    }

    #[test]
    fn zip_bomb_is_refused_as_limit_decode_bytes() {
        let valid = create_single_event_bundle().unwrap();
        let mut tar = Vec::new();
        flate2::read::GzDecoder::new(Cursor::new(&valid))
            .read_to_end(&mut tar)
            .unwrap();
        let limits = VerifyLimits {
            max_bundle_bytes: 5 * 1024 * 1024,
            max_decode_bytes: tar.len() as u64,
            ..Default::default()
        };
        let bomb = integrity_payloads::decode_bomb(&valid, limits).unwrap();
        assert!(
            (bomb.len() as u64) < limits.max_bundle_bytes,
            "compressed form must clear max_bundle_bytes"
        );
        let (status, code, _) = verify_payload(&bomb, limits);
        assert_eq!(status, AttackStatus::Blocked);
        assert_eq!(code.as_deref(), Some("LimitDecodeBytes"));
    }
}
