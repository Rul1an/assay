//! EXPERIMENTAL: import a skill supply-chain carrier into an evidence bundle.
//!
//! A carrier (`assay.skill_supply_chain.v0`) is a retained skill record: root identity, per-source
//! coverage flags, a closed reason vocabulary, and source-classed security signals. The contract is
//! pinned consumer-first (Assay-Harness `PLAN-SKILL-SUPPLY-CHAIN-SUFFICIENCY-2026Q3.md`; the Plimsoll
//! consumer pin fixes the same vocabularies byte-identically). This importer validates a single
//! supplied carrier fail-closed and writes it as one event into a bundle. Semantic verification is the
//! separate `evidence verify-skill-supply-chain` command.
//!
//! The import gate re-derives the verdict from the reason codes under the pinned worst-wins
//! precedence and rejects any carrier whose declared verdict disagrees: reason codes are decision
//! inputs, not disclosure. Dependency-detail fields (`declared_dependencies`, `reachable_dependencies`)
//! are carried opaquely in v0; the capture producer pins their shape when it lands.

use crate::exit_codes;
use anyhow::{bail, Context, Result};
use assay_evidence::bundle::BundleWriter;
use assay_evidence::types::{EvidenceEvent, ProducerMeta};
use chrono::{DateTime, Utc};
use clap::Args;
use serde_json::Value;
use std::fs;
use std::fs::File;
use std::path::PathBuf;

/// Schema id of the carrier this importer accepts (also the event type).
pub(crate) const CARRIER_SCHEMA: &str = "assay.skill_supply_chain.v0";
pub(crate) const CARRIER_EVENT_TYPE: &str = "assay.skill_supply_chain.v0";
const EVENT_SOURCE: &str = "urn:assay:external:skill-supply-chain";
const DEFAULT_RUN_ID: &str = "import-skill-supply-chain";

/// The five verdicts, in worst-wins precedence order.
const VERDICTS: &[&str] = &[
    "invalid",
    "transitive_risk_present",
    "review_incomplete",
    "review_ambiguous",
    "review_complete",
];
/// Closed reason vocabulary (byte-identical to the pinned consumer contract).
const INTEGRITY_REASONS: &[&str] = &["digest_mismatch", "unsafe_skill_path"];
const RISK_REASONS: &[&str] = &["hidden_package_inventory", "known_risk_signal_reachable"];
const COVERAGE_REASONS: &[&str] = &[
    "missing_package_version",
    "missing_lockfile_evidence",
    "missing_service_endpoint",
    "unversioned_cluster_member",
    "traversal_not_retained",
];
const AMBIGUITY_REASONS: &[&str] = &["unresolved_text_dependency"];
/// The five per-source coverage flags a producer must answer honestly.
const COVERAGE_KEYS: &[&str] = &[
    "front_matter",
    "body_text",
    "scripts",
    "lockfiles",
    "transitive_traversal",
];
const COVERAGE_VALUES: &[&str] = &["present", "not_present", "not_applicable"];
const SIGNAL_KINDS: &[&str] = &["occurrence", "absence"];

#[derive(Debug, Args, Clone)]
pub struct SkillSupplyChainArgs {
    /// Skill supply-chain carrier JSON artifact (`assay.skill_supply_chain.v0`)
    #[arg(long, value_name = "PATH")]
    pub carrier: PathBuf,

    /// Output Assay evidence bundle path (.tar.gz)
    #[arg(long, alias = "out", value_name = "PATH")]
    pub bundle_out: PathBuf,

    /// Assay import run id used for provenance and event ids
    #[arg(long, default_value = DEFAULT_RUN_ID)]
    pub run_id: String,

    /// Import timestamp for deterministic fixtures (RFC3339 UTC recommended)
    #[arg(long)]
    pub import_time: Option<String>,
}

pub fn cmd_skill_supply_chain(args: SkillSupplyChainArgs) -> Result<i32> {
    if args.run_id.contains(':') {
        bail!("run_id cannot contain ':' because event ids use run_id:seq");
    }
    let import_time = parse_import_time(args.import_time.as_deref())?;
    let producer = ProducerMeta {
        name: "assay-cli".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        git: option_env!("ASSAY_GIT_SHA").map(str::to_string),
    };

    let carrier_bytes = fs::read_to_string(&args.carrier)
        .with_context(|| format!("failed to read carrier {}", args.carrier.display()))?;
    let carrier: Value = serde_json::from_str(&carrier_bytes)
        .with_context(|| format!("failed to parse carrier {}", args.carrier.display()))?;

    validate_carrier(&carrier)?;

    let carrier_event =
        EvidenceEvent::new(CARRIER_EVENT_TYPE, EVENT_SOURCE, &args.run_id, 0, carrier)
            .with_time(import_time)
            .with_producer(&producer);

    let out_file = File::create(&args.bundle_out)
        .with_context(|| format!("failed to create bundle {}", args.bundle_out.display()))?;
    let mut writer = BundleWriter::new(out_file).with_producer(producer);
    writer.add_event(carrier_event);
    writer
        .finish()
        .with_context(|| format!("failed to write bundle {}", args.bundle_out.display()))?;

    eprintln!(
        "Imported skill supply-chain carrier to {}",
        args.bundle_out.display()
    );
    Ok(exit_codes::OK)
}

/// The verdict the reason codes require under the pinned worst-wins precedence.
pub(crate) fn expected_verdict(reasons: &[&str]) -> &'static str {
    if reasons.iter().any(|r| INTEGRITY_REASONS.contains(r)) {
        "invalid"
    } else if reasons.iter().any(|r| RISK_REASONS.contains(r)) {
        "transitive_risk_present"
    } else if reasons.iter().any(|r| COVERAGE_REASONS.contains(r)) {
        "review_incomplete"
    } else if reasons.iter().any(|r| AMBIGUITY_REASONS.contains(r)) {
        "review_ambiguous"
    } else {
        "review_complete"
    }
}

/// `true` when a root path is safe under the writer conventions: relative, POSIX separators, no
/// `..` components, no backslashes, not empty.
fn is_safe_skill_path(path: &str) -> bool {
    let has_drive_prefix = path.len() >= 2 && path.as_bytes()[1] == b':';
    let unsafe_path = path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || has_drive_prefix
        || path.split('/').any(|c| c == "..");
    !unsafe_path
}

/// Fail-closed carrier validation against the pinned contract. An incoherent carrier never reaches
/// the bundle: the declared verdict must re-derive from the reason codes, occurrence and absence
/// signals must corroborate the reasons, and coverage flags must be answered from the closed enum.
pub(crate) fn validate_carrier(carrier: &Value) -> Result<()> {
    let obj = carrier
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("carrier must be a JSON object"))?;

    match obj.get("schema").and_then(Value::as_str) {
        Some(CARRIER_SCHEMA) => {}
        Some(other) => bail!("carrier schema must be {CARRIER_SCHEMA:?}, got {other:?}"),
        None => bail!("carrier missing string schema"),
    }

    // Root identity: name + path-safety. An unsafe path may be retained as evidence, but only when
    // the record itself says so (unsafe_skill_path -> verdict invalid via worst-wins).
    let root = obj
        .get("root")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("carrier missing root object"))?;
    let root_name = root
        .get("name")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("carrier root.name must be a non-empty string"))?;
    let _ = root_name;
    let root_path = root
        .get("path")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("carrier root.path must be a non-empty string"))?;

    let verdict = obj
        .get("verdict")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("carrier missing string verdict"))?;
    if !VERDICTS.contains(&verdict) {
        bail!(
            "carrier verdict {verdict:?} is not one of {}",
            VERDICTS.join("|")
        );
    }

    let reasons_value = obj
        .get("reason_codes")
        .ok_or_else(|| anyhow::anyhow!("carrier missing reason_codes array"))?;
    let reasons: Vec<&str> = match reasons_value.as_array() {
        Some(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(item.as_str().ok_or_else(|| {
                    anyhow::anyhow!("carrier reason_codes must be an array of strings")
                })?);
            }
            out
        }
        None => bail!("carrier reason_codes must be an array of strings"),
    };
    for reason in &reasons {
        let known = INTEGRITY_REASONS.contains(reason)
            || RISK_REASONS.contains(reason)
            || COVERAGE_REASONS.contains(reason)
            || AMBIGUITY_REASONS.contains(reason);
        if !known {
            bail!("carrier reason code {reason:?} is not in the closed vocabulary; unknown reason codes reject the record");
        }
    }

    // The verdict is a derived value, never free-standing disclosure.
    let expected = expected_verdict(&reasons);
    if verdict != expected {
        bail!(
            "carrier verdict {verdict:?} does not match reason codes under worst-wins precedence (expected {expected:?})"
        );
    }

    // Path-safety coherence: an unsafe root path must be declared as such.
    if !is_safe_skill_path(root_path) && !reasons.contains(&"unsafe_skill_path") {
        bail!("carrier root.path {root_path:?} is unsafe but unsafe_skill_path is not declared");
    }

    // Coverage: exactly the five flags, each answered from the closed enum. Silence is not permitted.
    let coverage = obj
        .get("coverage")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("carrier missing coverage object"))?;
    for key in COVERAGE_KEYS {
        match coverage.get(*key).and_then(Value::as_str) {
            Some(value) if COVERAGE_VALUES.contains(&value) => {}
            Some(value) => bail!(
                "carrier coverage.{key} {value:?} is not one of {}",
                COVERAGE_VALUES.join("|")
            ),
            None => bail!(
                "carrier coverage.{key} must be one of {}",
                COVERAGE_VALUES.join("|")
            ),
        }
    }
    for key in coverage.keys() {
        if !COVERAGE_KEYS.contains(&key.as_str()) {
            bail!("carrier coverage has unknown key {key:?}");
        }
    }
    let traversal = coverage
        .get("transitive_traversal")
        .and_then(Value::as_str)
        .unwrap_or_default();
    // review_complete is legitimate either when the dependency graph was traversed (`present`) or when
    // there were no declared dependencies to traverse (`not_applicable`). It is only incoherent when
    // deps existed but traversal was not retained (`not_present`) — that must degrade to incomplete via
    // the `traversal_not_retained` reason, so review_complete + not_present is rejected.
    if verdict == "review_complete" && traversal == "not_present" {
        bail!(
            "carrier verdict review_complete is incoherent with transitive_traversal not_present; \
             declare traversal present or not_applicable, or degrade the verdict"
        );
    }

    // Signals: occurrence needs a risk reason; a known-signal reason needs an occurrence signal; an
    // absence statement needs a machine-readable justification and cannot appear with risk findings.
    let signals = match obj.get("signals") {
        None => Vec::new(),
        Some(Value::Array(items)) => items.iter().collect::<Vec<_>>(),
        Some(_) => bail!("carrier signals must be an array of objects"),
    };
    let mut occurrence_count = 0usize;
    let mut absence_count = 0usize;
    for signal in &signals {
        let sig = signal
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("carrier signals must be an array of objects"))?;
        let kind = sig
            .get("kind")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("carrier signal missing string kind"))?;
        if !SIGNAL_KINDS.contains(&kind) {
            bail!("carrier signal kind {kind:?} is not one of occurrence|absence");
        }
        sig.get("source_class")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                anyhow::anyhow!("carrier signal source_class must be a non-empty string")
            })?;
        if kind == "occurrence" {
            occurrence_count += 1;
        } else {
            absence_count += 1;
            sig.get("justification")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "carrier absence signal must carry a non-empty machine-readable justification"
                    )
                })?;
        }
    }
    let has_risk_reason = reasons.iter().any(|r| RISK_REASONS.contains(r));
    if occurrence_count > 0 && !has_risk_reason {
        bail!("carrier occurrence signal requires a risk reason code");
    }
    if reasons.contains(&"known_risk_signal_reachable") && occurrence_count == 0 {
        bail!("carrier known_risk_signal_reachable requires an occurrence signal");
    }
    if absence_count > 0 && has_risk_reason {
        bail!("carrier absence signal cannot appear with risk findings");
    }

    // The claim ceiling travels with the record.
    match obj.get("non_claims").and_then(Value::as_array) {
        Some(items) if !items.is_empty() && items.iter().all(|v| v.is_string()) => {}
        _ => bail!("carrier non_claims must be a non-empty array of strings"),
    }

    Ok(())
}

fn parse_import_time(value: Option<&str>) -> Result<DateTime<Utc>> {
    match value {
        Some(value) => Ok(DateTime::parse_from_rfc3339(value)
            .with_context(|| format!("invalid --import-time {value:?}; expected RFC3339"))?
            .with_timezone(&Utc)),
        None => Ok(Utc::now()),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use assay_evidence::bundle::BundleReader;
    use serde_json::json;

    pub(crate) fn sample_carrier() -> Value {
        json!({
            "schema": CARRIER_SCHEMA,
            "root": {"name": "release-notes", "path": "skills/release-notes"},
            "verdict": "review_complete",
            "reason_codes": [],
            "coverage": {
                "front_matter": "present",
                "body_text": "present",
                "scripts": "present",
                "lockfiles": "present",
                "transitive_traversal": "present"
            },
            "signals": [],
            "non_claims": [
                "review_complete_is_not_skill_safe",
                "verdict_covers_one_root_at_one_capture_time"
            ]
        })
    }

    pub(crate) fn run_import(
        carrier: &Value,
        run_id: &str,
    ) -> Result<(std::path::PathBuf, tempfile::TempDir)> {
        let dir = tempfile::tempdir().unwrap();
        let carrier_path = dir.path().join("carrier.json");
        let out = dir.path().join("ssc.tar.gz");
        fs::write(
            &carrier_path,
            serde_json::to_string_pretty(carrier).unwrap(),
        )
        .unwrap();
        cmd_skill_supply_chain(SkillSupplyChainArgs {
            carrier: carrier_path,
            bundle_out: out.clone(),
            run_id: run_id.to_string(),
            import_time: Some("2026-07-02T00:00:00Z".to_string()),
        })
        .map(|_| (out, dir))
    }

    #[test]
    fn import_writes_single_carrier_event_bundle() {
        let (out, _dir) = run_import(&sample_carrier(), "ssc_test").unwrap();
        let reader = BundleReader::open(File::open(&out).unwrap()).unwrap();
        assert_eq!(reader.manifest().event_count, 1);
        let events = reader.events_vec().unwrap();
        assert_eq!(events[0].type_, CARRIER_EVENT_TYPE);
        assert_eq!(events[0].payload["verdict"], json!("review_complete"));
    }

    #[test]
    fn import_accepts_all_vector_families() {
        // The lab vector families, expressed as coherent carriers.
        let mut incomplete_lockfile = sample_carrier();
        incomplete_lockfile["verdict"] = json!("review_incomplete");
        incomplete_lockfile["reason_codes"] =
            json!(["missing_package_version", "missing_lockfile_evidence"]);
        incomplete_lockfile["coverage"]["lockfiles"] = json!("not_present");

        let mut incomplete_endpoint = sample_carrier();
        incomplete_endpoint["verdict"] = json!("review_incomplete");
        incomplete_endpoint["reason_codes"] = json!(["missing_service_endpoint"]);

        let mut incomplete_cluster = sample_carrier();
        incomplete_cluster["verdict"] = json!("review_incomplete");
        incomplete_cluster["reason_codes"] = json!(["unversioned_cluster_member"]);

        let mut ambiguous = sample_carrier();
        ambiguous["verdict"] = json!("review_ambiguous");
        ambiguous["reason_codes"] = json!(["unresolved_text_dependency"]);

        let mut hidden_inventory = sample_carrier();
        hidden_inventory["verdict"] = json!("transitive_risk_present");
        hidden_inventory["reason_codes"] = json!(["hidden_package_inventory"]);

        let mut known_signal = sample_carrier();
        known_signal["verdict"] = json!("transitive_risk_present");
        known_signal["reason_codes"] = json!(["known_risk_signal_reachable"]);
        known_signal["signals"] = json!([
            {"kind": "occurrence", "source_class": "registry_scanner_report"}
        ]);

        let mut invalid_digest = sample_carrier();
        invalid_digest["verdict"] = json!("invalid");
        invalid_digest["reason_codes"] = json!(["digest_mismatch"]);

        // Risk reports at any coverage level: risk + missing coverage stays risk.
        let mut risk_partial = sample_carrier();
        risk_partial["verdict"] = json!("transitive_risk_present");
        risk_partial["reason_codes"] =
            json!(["hidden_package_inventory", "missing_lockfile_evidence"]);
        risk_partial["coverage"]["lockfiles"] = json!("not_present");
        risk_partial["coverage"]["transitive_traversal"] = json!("not_present");

        for (name, carrier) in [
            ("incomplete_lockfile", incomplete_lockfile),
            ("incomplete_endpoint", incomplete_endpoint),
            ("incomplete_cluster", incomplete_cluster),
            ("ambiguous", ambiguous),
            ("hidden_inventory", hidden_inventory),
            ("known_signal", known_signal),
            ("invalid_digest", invalid_digest),
            ("risk_partial", risk_partial),
        ] {
            run_import(&carrier, "ssc_test")
                .unwrap_or_else(|e| panic!("family {name} must import: {e}"));
        }
    }

    #[test]
    fn import_rejects_unknown_reason_code() {
        let mut carrier = sample_carrier();
        carrier["verdict"] = json!("review_incomplete");
        carrier["reason_codes"] = json!(["novel_reason"]);
        let err = run_import(&carrier, "ssc_test").unwrap_err();
        assert!(err.to_string().contains("closed vocabulary"));
    }

    #[test]
    fn import_rejects_verdict_reason_mismatch_both_directions() {
        // Coverage reason relabelled as risk.
        let mut relabel = sample_carrier();
        relabel["verdict"] = json!("transitive_risk_present");
        relabel["reason_codes"] = json!(["missing_lockfile_evidence"]);
        let err = run_import(&relabel, "ssc_test").unwrap_err();
        assert!(err.to_string().contains("worst-wins"));

        // Risk downgraded for coverage.
        let mut downgrade = sample_carrier();
        downgrade["verdict"] = json!("review_incomplete");
        downgrade["reason_codes"] =
            json!(["hidden_package_inventory", "missing_lockfile_evidence"]);
        let err = run_import(&downgrade, "ssc_test").unwrap_err();
        assert!(err.to_string().contains("worst-wins"));

        // Integrity dominates risk.
        let mut integrity = sample_carrier();
        integrity["verdict"] = json!("transitive_risk_present");
        integrity["reason_codes"] = json!(["digest_mismatch", "hidden_package_inventory"]);
        let err = run_import(&integrity, "ssc_test").unwrap_err();
        assert!(err.to_string().contains("expected \"invalid\""));
    }

    #[test]
    fn import_rejects_signal_incoherence() {
        // Occurrence signal without a risk reason.
        let mut occurrence = sample_carrier();
        occurrence["signals"] =
            json!([{"kind": "occurrence", "source_class": "registry_scanner_report"}]);
        let err = run_import(&occurrence, "ssc_test").unwrap_err();
        assert!(err.to_string().contains("requires a risk reason code"));

        // known_risk_signal_reachable without an occurrence signal.
        let mut no_signal = sample_carrier();
        no_signal["verdict"] = json!("transitive_risk_present");
        no_signal["reason_codes"] = json!(["known_risk_signal_reachable"]);
        let err = run_import(&no_signal, "ssc_test").unwrap_err();
        assert!(err.to_string().contains("requires an occurrence signal"));

        // Absence signal alongside risk findings.
        let mut contradiction = sample_carrier();
        contradiction["verdict"] = json!("transitive_risk_present");
        contradiction["reason_codes"] = json!(["hidden_package_inventory"]);
        contradiction["signals"] = json!([
            {"kind": "absence", "source_class": "boundary_observed",
             "justification": "reviewed_boundary_fully_traversed"}
        ]);
        let err = run_import(&contradiction, "ssc_test").unwrap_err();
        assert!(err.to_string().contains("cannot appear with risk findings"));

        // Bare absence statement (no justification).
        let mut bare = sample_carrier();
        bare["signals"] = json!([{"kind": "absence", "source_class": "boundary_observed"}]);
        let err = run_import(&bare, "ssc_test").unwrap_err();
        assert!(err.to_string().contains("machine-readable justification"));
    }

    #[test]
    fn import_rejects_dishonest_coverage() {
        // review_complete without retained traversal.
        let mut no_traversal = sample_carrier();
        no_traversal["coverage"]["transitive_traversal"] = json!("not_present");
        let err = run_import(&no_traversal, "ssc_test").unwrap_err();
        assert!(err.to_string().contains("transitive_traversal"));

        // Out-of-enum coverage value.
        let mut bad_value = sample_carrier();
        bad_value["coverage"]["lockfiles"] = json!("unknown");
        let err = run_import(&bad_value, "ssc_test").unwrap_err();
        assert!(err.to_string().contains("coverage.lockfiles"));

        // Unknown coverage key.
        let mut extra_key = sample_carrier();
        extra_key["coverage"]["network"] = json!("present");
        let err = run_import(&extra_key, "ssc_test").unwrap_err();
        assert!(err.to_string().contains("unknown key"));

        // Missing flag (silence is not permitted).
        let mut silent = sample_carrier();
        silent["coverage"]
            .as_object_mut()
            .unwrap()
            .remove("scripts");
        let err = run_import(&silent, "ssc_test").unwrap_err();
        assert!(err.to_string().contains("coverage.scripts"));
    }

    #[test]
    fn import_rejects_undeclared_unsafe_root_path() {
        for bad in ["/abs/skill", "skills/../escape", "skills\\win"] {
            let mut carrier = sample_carrier();
            carrier["root"]["path"] = json!(bad);
            let err = run_import(&carrier, "ssc_test").unwrap_err();
            assert!(
                err.to_string().contains("unsafe"),
                "path {bad:?} must be rejected: {err}"
            );
        }

        // The same path IS importable when the record declares it and the verdict follows.
        let mut declared = sample_carrier();
        declared["root"]["path"] = json!("/abs/skill");
        declared["verdict"] = json!("invalid");
        declared["reason_codes"] = json!(["unsafe_skill_path"]);
        run_import(&declared, "ssc_test").unwrap();
    }

    #[test]
    fn import_rejects_missing_non_claims_and_bad_run_id() {
        let mut carrier = sample_carrier();
        carrier["non_claims"] = json!([]);
        let err = run_import(&carrier, "ssc_test").unwrap_err();
        assert!(err.to_string().contains("non_claims"));

        let err = run_import(&sample_carrier(), "bad:run").unwrap_err();
        assert!(err.to_string().contains("run_id cannot contain ':'"));
    }
}
