//! EXPERIMENTAL: semantic verification of skill supply-chain carriers inside an evidence bundle.
//!
//! `BundleReader::open` verifies bundle integrity (manifest hashes + Merkle root). This command layers
//! the pinned carrier contract on top: every `assay.skill_supply_chain.v0` event is re-validated
//! fail-closed (closed vocabularies, worst-wins verdict recompute, coverage honesty, signal
//! coherence), and duplicate root identities fail because the reviewed subject would be ambiguous.
//! The report names what is deliberately NOT claimed: verification proves the retained record is
//! coherent and recomputable, never that a skill is safe.

use crate::exit_codes;
use anyhow::{Context, Result};
use assay_evidence::bundle::BundleReader;
use assay_evidence::types::EvidenceEvent;
use clap::{Args, ValueEnum};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::fs::File;
use std::path::PathBuf;

use super::skill_supply_chain::{expected_verdict, validate_carrier, CARRIER_EVENT_TYPE};

const CLAIMS_NOT_MADE: &[&str] = &[
    "skill_safety_or_maliciousness",
    "registry_wide_state",
    "runtime_behavior",
    "other_versions_or_skills",
    "absence_of_risk_beyond_reviewed_boundary",
];

#[derive(Debug, Args, Clone)]
pub struct VerifySkillSupplyChainArgs {
    /// Evidence bundle (.tar.gz) with skill supply-chain carrier events
    #[arg(value_name = "BUNDLE")]
    pub bundle: PathBuf,

    /// Output format
    #[arg(long, value_enum, default_value_t = VerifyFormat::Table)]
    pub format: VerifyFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum VerifyFormat {
    Json,
    Table,
}

#[derive(Debug, Serialize)]
struct Report {
    schema: &'static str,
    ok: bool,
    carrier_count: usize,
    verified_carriers: usize,
    checks: Vec<Check>,
    claims_not_made: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct Check {
    id: String,
    ok: bool,
    detail: String,
}

pub fn cmd_verify_skill_supply_chain(args: VerifySkillSupplyChainArgs) -> Result<i32> {
    let file = File::open(&args.bundle)
        .with_context(|| format!("failed to open bundle {}", args.bundle.display()))?;
    // Verify-before-read: bundle integrity first, carrier semantics second.
    let reader = BundleReader::open(file).context("bundle integrity verification failed")?;
    let events = reader
        .events_vec()
        .context("failed to read bundle events")?;

    let report = build_report(&events);
    match args.format {
        VerifyFormat::Json => println!("{}", serde_json::to_string_pretty(&report)?),
        VerifyFormat::Table => print_table(&report),
    }
    Ok(if report.ok { exit_codes::OK } else { 2 })
}

fn build_report(events: &[EvidenceEvent]) -> Report {
    let mut checks = Vec::new();
    let mut carrier_count = 0usize;
    let mut verified = 0usize;
    let mut roots: HashSet<(String, String)> = HashSet::new();

    for ev in events.iter().filter(|e| e.type_ == CARRIER_EVENT_TYPE) {
        carrier_count += 1;
        let id = format!("carrier_{}", ev.id);

        match validate_carrier(&ev.payload) {
            Ok(()) => {
                // Independent recompute: the reported verdict must re-derive from the reasons.
                let reasons: Vec<&str> = ev.payload["reason_codes"]
                    .as_array()
                    .map(|items| items.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                let expected = expected_verdict(&reasons);
                let verdict = ev.payload["verdict"].as_str().unwrap_or_default();
                if verdict == expected {
                    verified += 1;
                    checks.push(Check {
                        id: format!("{id}_contract"),
                        ok: true,
                        detail: format!(
                            "verdict {verdict} recomputes from {} reason code(s)",
                            reasons.len()
                        ),
                    });
                } else {
                    checks.push(Check {
                        id: format!("{id}_contract"),
                        ok: false,
                        detail: format!(
                            "verdict {verdict:?} does not recompute (expected {expected:?})"
                        ),
                    });
                }

                let root_name = ev.payload["root"]["name"].as_str().unwrap_or_default();
                let root_path = ev.payload["root"]["path"].as_str().unwrap_or_default();
                if !roots.insert((root_name.to_string(), root_path.to_string())) {
                    checks.push(Check {
                        id: "duplicate_root_identity".to_string(),
                        ok: false,
                        detail: format!(
                            "more than one carrier reviews root {root_name:?} at {root_path:?}"
                        ),
                    });
                }
            }
            Err(err) => checks.push(Check {
                id: format!("{id}_contract"),
                ok: false,
                detail: err.to_string(),
            }),
        }
    }

    if carrier_count == 0 {
        checks.push(Check {
            id: "carrier_present".to_string(),
            ok: false,
            detail: format!("bundle contains no {CARRIER_EVENT_TYPE} events"),
        });
    }

    let ok = carrier_count > 0 && checks.iter().all(|c| c.ok);
    Report {
        schema: "assay.skill_supply_chain.verify_report.v0",
        ok,
        carrier_count,
        verified_carriers: verified,
        checks,
        claims_not_made: CLAIMS_NOT_MADE.to_vec(),
    }
}

fn print_table(report: &Report) {
    println!(
        "skill supply-chain verification: {}",
        if report.ok { "OK" } else { "FAILED" }
    );
    println!(
        "carriers: {} ({} verified)",
        report.carrier_count, report.verified_carriers
    );
    for check in &report.checks {
        println!(
            "  [{}] {}: {}",
            if check.ok { "ok" } else { "FAIL" },
            check.id,
            check.detail
        );
    }
    println!("claims not made: {}", report.claims_not_made.join(", "));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::commands::evidence::skill_supply_chain::tests::{run_import, sample_carrier};
    use assay_evidence::bundle::BundleWriter;
    use assay_evidence::types::{EvidenceEvent, ProducerMeta};
    use serde_json::json;

    fn verify(bundle: &std::path::Path) -> Result<i32> {
        cmd_verify_skill_supply_chain(VerifySkillSupplyChainArgs {
            bundle: bundle.to_path_buf(),
            format: VerifyFormat::Json,
        })
    }

    fn write_bundle(payloads: Vec<Value>) -> (std::path::PathBuf, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("ssc.tar.gz");
        let producer = ProducerMeta {
            name: "test".to_string(),
            version: "0".to_string(),
            git: None,
        };
        let mut writer =
            BundleWriter::new(File::create(&out).unwrap()).with_producer(producer.clone());
        for (seq, payload) in payloads.into_iter().enumerate() {
            writer.add_event(
                EvidenceEvent::new(
                    CARRIER_EVENT_TYPE,
                    "urn:assay:test",
                    "verify_test",
                    seq as u64,
                    payload,
                )
                .with_producer(&producer),
            );
        }
        writer.finish().unwrap();
        (out, dir)
    }

    #[test]
    fn verify_round_trip_from_import_is_ok() {
        let (bundle, _dir) = run_import(&sample_carrier(), "ssc_verify").unwrap();
        assert_eq!(verify(&bundle).unwrap(), 0);
    }

    #[test]
    fn verify_fails_on_incoherent_carrier_in_bundle() {
        // A carrier written around the import gate (verdict/reason mismatch) fails verification.
        let mut bad = sample_carrier();
        bad["verdict"] = json!("transitive_risk_present");
        let (bundle, _dir) = write_bundle(vec![bad]);
        assert_eq!(verify(&bundle).unwrap(), 2);
    }

    #[test]
    fn verify_fails_on_duplicate_root_identity() {
        let (bundle, _dir) = write_bundle(vec![sample_carrier(), sample_carrier()]);
        assert_eq!(verify(&bundle).unwrap(), 2);
    }

    #[test]
    fn verify_fails_on_bundle_without_carriers() {
        // A valid bundle whose only event is a different type: no reviewable carrier -> fail.
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("other.tar.gz");
        let producer = ProducerMeta {
            name: "test".to_string(),
            version: "0".to_string(),
            git: None,
        };
        let mut writer =
            BundleWriter::new(File::create(&out).unwrap()).with_producer(producer.clone());
        writer.add_event(
            EvidenceEvent::new(
                "assay.other.v0",
                "urn:assay:test",
                "verify_test",
                0,
                json!({}),
            )
            .with_producer(&producer),
        );
        writer.finish().unwrap();
        assert_eq!(verify(&out).unwrap(), 2);
    }
}
