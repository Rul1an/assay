//! EXPERIMENTAL: adapt a SARIF 2.1.0 scanner report onto a skill supply-chain carrier.
//!
//! A static-analysis scanner (SkillSpector, Snyk, ClawScan, any SARIF emitter) observes what it can
//! reach; its report is OCCURRENCE-scoped by source class. This adapter reads a SARIF log and attaches
//! every `error`/`warning` result to the carrier as a source-classed `scanner_findings` entry for the
//! reviewer. Crucially, a raw scanner finding is NOT automatically a verdict-bearing risk: only a
//! result whose `ruleId` is in the caller's `--known-risk-rule` set is promoted to a contract
//! `signals` occurrence entry, which adds `known_risk_signal_reachable` so the verdict recomputes
//! upward under worst-wins. This preserves the pinned contract (a verdict-bearing occurrence signal
//! always corresponds to a risk reason) and the source-class discipline (a scan attests presence, never
//! absence, and never downgrades a verdict). The augmented carrier is re-validated with the import
//! gate, so a scan can never produce an incoherent carrier.

use super::skill_supply_chain::{expected_verdict, validate_carrier};
use crate::exit_codes;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

const SIGNAL_SOURCE_CLASS: &str = "static_analysis_scanner";

#[derive(Debug, clap::Args, Clone)]
pub struct AdaptSkillScanArgs {
    /// Existing assay.skill_supply_chain.v0 carrier to augment
    #[arg(long, value_name = "PATH")]
    pub carrier: PathBuf,

    /// SARIF 2.1.0 scanner report
    #[arg(long, value_name = "PATH")]
    pub sarif: PathBuf,

    /// Rule id that marks a result as a KNOWN reachable risk (repeatable)
    #[arg(long = "known-risk-rule", value_name = "RULE_ID")]
    pub known_risk_rules: Vec<String>,

    /// Where to write the augmented carrier (stdout if omitted)
    #[arg(long, value_name = "PATH")]
    pub out: Option<PathBuf>,
}

pub fn cmd_adapt_skill_scan(args: AdaptSkillScanArgs) -> Result<i32> {
    let carrier_bytes = fs::read_to_string(&args.carrier)
        .with_context(|| format!("failed to read carrier {}", args.carrier.display()))?;
    let mut carrier: Value = serde_json::from_str(&carrier_bytes)
        .with_context(|| format!("failed to parse carrier {}", args.carrier.display()))?;
    // The input carrier must itself be coherent before we touch it.
    validate_carrier(&carrier)
        .context("input carrier is not a valid skill supply-chain carrier")?;

    let sarif_bytes = fs::read_to_string(&args.sarif)
        .with_context(|| format!("failed to read SARIF {}", args.sarif.display()))?;
    let sarif: Value = serde_json::from_str(&sarif_bytes)
        .with_context(|| format!("failed to parse SARIF {}", args.sarif.display()))?;

    let known: BTreeSet<&str> = args.known_risk_rules.iter().map(String::as_str).collect();
    let scan = sarif_to_findings(&sarif, &known)?;

    augment(&mut carrier, scan)?;
    validate_carrier(&carrier).context("augmented carrier failed self-validation (bug)")?;

    let json = serde_json::to_string_pretty(&carrier)?;
    match &args.out {
        Some(path) => {
            fs::write(path, format!("{json}\n"))
                .with_context(|| format!("failed to write {}", path.display()))?;
            eprintln!("Wrote augmented carrier to {}", path.display());
        }
        None => println!("{json}"),
    }
    Ok(exit_codes::OK)
}

/// The parsed scanner report: every error/warning as a reviewer-facing finding, plus the subset that
/// matched the known-risk rule set (promoted to verdict-bearing occurrence signals).
struct Scan {
    findings: Vec<Value>,
    known_risk_signals: Vec<Value>,
}

/// Read SARIF `error`/`warning` results. `note`/`none` are informational and skipped. Every kept
/// result becomes a `scanner_findings` entry; a result whose ruleId is in `known` additionally becomes
/// a contract occurrence signal.
fn sarif_to_findings(sarif: &Value, known: &BTreeSet<&str>) -> Result<Scan> {
    if sarif.get("version").and_then(Value::as_str) != Some("2.1.0") {
        bail!("expected a SARIF 2.1.0 log (\"version\":\"2.1.0\")");
    }
    let runs = sarif
        .get("runs")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("SARIF log missing runs array"))?;

    let mut findings = Vec::new();
    let mut known_risk_signals = Vec::new();
    for run in runs {
        let tool = run
            .get("tool")
            .and_then(|t| t.get("driver"))
            .and_then(|d| d.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        for result in run
            .get("results")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let level = result
                .get("level")
                .and_then(Value::as_str)
                .unwrap_or("warning");
            if level != "error" && level != "warning" {
                continue;
            }
            let rule_id = result.get("ruleId").and_then(Value::as_str).unwrap_or("");
            let message = result
                .get("message")
                .and_then(|m| m.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let is_known = known.contains(rule_id);
            findings.push(json!({
                "source_class": SIGNAL_SOURCE_CLASS,
                "tool": tool,
                "rule_id": rule_id,
                "level": level,
                "message": message,
                "known_risk": is_known,
            }));
            if is_known {
                known_risk_signals.push(json!({
                    "kind": "occurrence",
                    "source_class": SIGNAL_SOURCE_CLASS,
                    "tool": tool,
                    "rule_id": rule_id,
                    "message": message,
                }));
            }
        }
    }
    Ok(Scan {
        findings,
        known_risk_signals,
    })
}

/// Attach the scan to the carrier. Raw findings go to `scanner_findings` (informational, source-classed,
/// never verdict-bearing). Known-risk results additionally become contract occurrence signals and add
/// `known_risk_signal_reachable`, so the verdict recomputes upward under worst-wins — never downward.
fn augment(carrier: &mut Value, scan: Scan) -> Result<()> {
    let obj = carrier
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("carrier must be a JSON object"))?;

    if !scan.findings.is_empty() {
        let mut existing: Vec<Value> = obj
            .get("scanner_findings")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        existing.extend(scan.findings);
        obj.insert("scanner_findings".into(), Value::Array(existing));
    }

    if !scan.known_risk_signals.is_empty() {
        // A live known-risk occurrence contradicts any prior "no reachable signal" absence claim, so
        // drop absence signals when we add occurrence ones.
        let mut kept: Vec<Value> = obj
            .get("signals")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter(|s| s.get("kind").and_then(Value::as_str) != Some("absence"))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        kept.extend(scan.known_risk_signals);
        obj.insert("signals".into(), Value::Array(kept));

        let mut reasons: BTreeSet<String> = obj
            .get("reason_codes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        reasons.insert("known_risk_signal_reachable".to_string());
        let reasons: Vec<String> = reasons.into_iter().collect();
        let reason_refs: Vec<&str> = reasons.iter().map(String::as_str).collect();
        let new_verdict = expected_verdict(&reason_refs);
        obj.insert("reason_codes".into(), json!(reasons));
        obj.insert("verdict".into(), json!(new_verdict));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base_carrier() -> Value {
        json!({
            "schema": "assay.skill_supply_chain.v0",
            "root": {"name": "s", "path": "skills/s"},
            "verdict": "review_complete",
            "reason_codes": [],
            "coverage": {
                "front_matter": "present", "body_text": "present", "scripts": "present",
                "lockfiles": "present", "transitive_traversal": "present"
            },
            "signals": [],
            "non_claims": ["review_complete_is_not_skill_safe"]
        })
    }

    fn sarif(results: Value) -> Value {
        json!({
            "version": "2.1.0",
            "runs": [{"tool": {"driver": {"name": "SkillSpector"}}, "results": results}]
        })
    }

    fn run(carrier: &Value, sarif: &Value, known: &[&str]) -> (i32, Value) {
        let dir = tempfile::tempdir().unwrap();
        let cp = dir.path().join("c.json");
        let sp = dir.path().join("s.sarif");
        let out = dir.path().join("out.json");
        fs::write(&cp, serde_json::to_string(carrier).unwrap()).unwrap();
        fs::write(&sp, serde_json::to_string(sarif).unwrap()).unwrap();
        let rc = cmd_adapt_skill_scan(AdaptSkillScanArgs {
            carrier: cp,
            sarif: sp,
            known_risk_rules: known.iter().map(|s| s.to_string()).collect(),
            out: Some(out.clone()),
        })
        .unwrap();
        let augmented = serde_json::from_str(&fs::read_to_string(&out).unwrap()).unwrap();
        (rc, augmented)
    }

    #[test]
    fn non_known_warning_attaches_as_scanner_finding_without_changing_verdict() {
        let s = sarif(json!([
            {"ruleId": "OVERBROAD_FS", "level": "warning", "message": {"text": "writes /etc"}}
        ]));
        let (rc, out) = run(&base_carrier(), &s, &[]);
        assert_eq!(rc, 0);
        // Raw scanner finding is attached for the reviewer, source-classed, but NOT a contract signal.
        let findings = out["scanner_findings"].as_array().unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0]["source_class"], "static_analysis_scanner");
        assert_eq!(findings[0]["known_risk"], false);
        assert_eq!(out["signals"].as_array().unwrap().len(), 0);
        // A non-known finding does not flip the verdict.
        assert_eq!(out["verdict"], "review_complete");
    }

    #[test]
    fn known_risk_rule_elevates_verdict_to_transitive_risk_present() {
        let s = sarif(json!([
            {"ruleId": "MALICIOUS_SKILL", "level": "error", "message": {"text": "reaches known-bad"}}
        ]));
        let (rc, out) = run(&base_carrier(), &s, &["MALICIOUS_SKILL"]);
        assert_eq!(rc, 0);
        assert_eq!(out["verdict"], "transitive_risk_present");
        let reasons: Vec<&str> = out["reason_codes"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert!(reasons.contains(&"known_risk_signal_reachable"));
        // Promoted to a verdict-bearing occurrence signal, AND recorded as a raw finding.
        assert_eq!(out["signals"].as_array().unwrap().len(), 1);
        assert_eq!(out["signals"][0]["kind"], "occurrence");
        assert_eq!(out["scanner_findings"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn note_level_results_are_skipped() {
        let s = sarif(json!([
            {"ruleId": "STYLE", "level": "note", "message": {"text": "minor"}}
        ]));
        let (_rc, out) = run(&base_carrier(), &s, &[]);
        assert!(
            out.get("scanner_findings").is_none()
                || out["scanner_findings"].as_array().unwrap().is_empty()
        );
        assert_eq!(out["signals"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn scan_never_downgrades_an_incomplete_carrier() {
        let mut c = base_carrier();
        c["verdict"] = json!("review_incomplete");
        c["reason_codes"] = json!(["missing_lockfile_evidence"]);
        c["coverage"]["lockfiles"] = json!("not_present");
        let s = sarif(json!([
            {"ruleId": "X", "level": "warning", "message": {"text": "y"}}
        ]));
        let (_rc, out) = run(&c, &s, &[]);
        // Still at least incomplete; a benign scan finding cannot clear a coverage gap.
        assert_eq!(out["verdict"], "review_incomplete");
    }

    #[test]
    fn known_risk_dominates_coverage_gap_under_worst_wins() {
        let mut c = base_carrier();
        c["verdict"] = json!("review_incomplete");
        c["reason_codes"] = json!(["missing_lockfile_evidence"]);
        c["coverage"]["lockfiles"] = json!("not_present");
        let s = sarif(json!([
            {"ruleId": "BAD", "level": "error", "message": {"text": "z"}}
        ]));
        let (_rc, out) = run(&c, &s, &["BAD"]);
        assert_eq!(out["verdict"], "transitive_risk_present");
    }

    #[test]
    fn known_risk_drops_prior_absence_claim() {
        let mut c = base_carrier();
        c["signals"] = json!([
            {"kind": "absence", "source_class": "boundary_observed",
             "justification": "reviewed_boundary_fully_traversed"}
        ]);
        let s = sarif(json!([
            {"ruleId": "BAD", "level": "error", "message": {"text": "y"}}
        ]));
        let (rc, out) = run(&c, &s, &["BAD"]);
        assert_eq!(rc, 0);
        // The stale absence claim is gone; only the live occurrence signal remains.
        let signals = out["signals"].as_array().unwrap();
        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0]["kind"], "occurrence");
    }

    #[test]
    fn non_known_finding_does_not_disturb_a_valid_absence_claim() {
        // A benign scanner finding is attached as raw evidence but must not contradict/drop an
        // existing absence claim, because it is not a verdict-bearing occurrence signal.
        let mut c = base_carrier();
        c["signals"] = json!([
            {"kind": "absence", "source_class": "boundary_observed",
             "justification": "reviewed_boundary_fully_traversed"}
        ]);
        let s = sarif(json!([
            {"ruleId": "STYLE", "level": "warning", "message": {"text": "minor"}}
        ]));
        let (rc, out) = run(&c, &s, &[]);
        assert_eq!(rc, 0);
        assert_eq!(out["signals"].as_array().unwrap().len(), 1);
        assert_eq!(out["signals"][0]["kind"], "absence");
        assert_eq!(out["scanner_findings"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn rejects_non_sarif_211() {
        let s = json!({"version": "2.0.0", "runs": []});
        let dir = tempfile::tempdir().unwrap();
        let cp = dir.path().join("c.json");
        let sp = dir.path().join("s.sarif");
        fs::write(&cp, serde_json::to_string(&base_carrier()).unwrap()).unwrap();
        fs::write(&sp, serde_json::to_string(&s).unwrap()).unwrap();
        let err = cmd_adapt_skill_scan(AdaptSkillScanArgs {
            carrier: cp,
            sarif: sp,
            known_risk_rules: vec![],
            out: None,
        })
        .unwrap_err();
        assert!(err.to_string().contains("SARIF 2.1.0"));
    }
}
