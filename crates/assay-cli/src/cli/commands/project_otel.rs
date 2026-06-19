//! `assay project-otel` — read-only projection of assay evidence into the OTel GenAI + OpenInference
//! view.
//!
//! Guardrail: this command must never be smarter than the library projector. It reads files,
//! deserializes JSON, calls `assay_core::otel::projection::project`, and writes the result. All
//! projection semantics live in `assay_core`, so there is exactly one projection truth — never a
//! second, divergent CLI projection.

use std::path::Path;

use serde_json::Value;

use crate::cli::args::ProjectOtelArgs;
use crate::exit_codes::{EXIT_CONFIG_ERROR, EXIT_SUCCESS};

fn read_json(path: &Path) -> anyhow::Result<Value> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|e| anyhow::anyhow!("invalid JSON in {}: {e}", path.display()))
}

/// Project the supplied artifacts. Input/IO errors are reported on stderr and return
/// `EXIT_CONFIG_ERROR`, leaving stdout empty; on success the projection is the only thing written to
/// stdout (or to `--out`), as pure JSON.
pub fn run(args: ProjectOtelArgs) -> anyhow::Result<i32> {
    if let Some(bundle) = args.evidence_bundle.as_deref() {
        return run_tool_decision_truth(bundle, args.out.as_deref());
    }
    let Some(capability_surface_path) = args.capability_surface.as_deref() else {
        eprintln!("error: one of --capability-surface or --evidence-bundle is required");
        return Ok(EXIT_CONFIG_ERROR);
    };
    let capability_surface = match read_json(capability_surface_path) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {e}");
            return Ok(EXIT_CONFIG_ERROR);
        }
    };
    let observation_health = match args
        .observation_health
        .as_deref()
        .map(read_json)
        .transpose()
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {e}");
            return Ok(EXIT_CONFIG_ERROR);
        }
    };
    let enforcement_health = match args
        .enforcement_health
        .as_deref()
        .map(read_json)
        .transpose()
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {e}");
            return Ok(EXIT_CONFIG_ERROR);
        }
    };

    let projection = assay_core::otel::projection::project(
        &capability_surface,
        observation_health.as_ref(),
        enforcement_health.as_ref(),
    );
    let json = serde_json::to_string_pretty(&projection)?;

    match &args.out {
        Some(path) => {
            if let Err(e) = std::fs::write(path, format!("{json}\n")) {
                eprintln!("error: cannot write {}: {e}", path.display());
                return Ok(EXIT_CONFIG_ERROR);
            }
        }
        None => println!("{json}"),
    }
    Ok(EXIT_SUCCESS)
}

/// Project verified tool-decision-truth recipe rows from an evidence bundle. The bundle is verified in
/// full FIRST; if the semantic report is not `ok`, nothing is serialized or written (not even to
/// `--out`). This stays a view over verified evidence, never a best-effort trace extractor.
fn run_tool_decision_truth(bundle: &Path, out: Option<&Path>) -> anyhow::Result<i32> {
    let file = match std::fs::File::open(bundle) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: cannot open bundle {}: {e}", bundle.display());
            return Ok(EXIT_CONFIG_ERROR);
        }
    };
    // BundleReader::open verifies bundle integrity (manifest hashes + Merkle root).
    let reader = match assay_evidence::bundle::BundleReader::open(file) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: bundle integrity verification failed: {e}");
            return Ok(EXIT_CONFIG_ERROR);
        }
    };
    let events = match reader.events_vec() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: cannot read bundle events: {e}");
            return Ok(EXIT_CONFIG_ERROR);
        }
    };

    // Verify FULLY before serializing anything. Projection runs only over verified pairs.
    let (report, verified) =
        crate::cli::commands::evidence::verify_tool_decision_truth::verify_and_collect(&events);
    if !report.ok {
        eprintln!("error: tool-decision-truth verification failed; refusing to project unverified evidence");
        for check in report.checks.iter().filter(|c| !c.ok) {
            eprintln!("  fail: {} — {}", check.id, check.detail);
        }
        return Ok(EXIT_CONFIG_ERROR);
    }

    let projection = assay_core::otel::projection::project_tool_decision_truth(&verified);
    let json = serde_json::to_string_pretty(&projection)?;
    match out {
        Some(path) => {
            if let Err(e) = std::fs::write(path, format!("{json}\n")) {
                eprintln!("error: cannot write {}: {e}", path.display());
                return Ok(EXIT_CONFIG_ERROR);
            }
        }
        None => println!("{json}"),
    }
    Ok(EXIT_SUCCESS)
}

#[cfg(test)]
mod tdt_bundle_tests {
    use super::*;
    use assay_core::mcp::policy::McpPolicy;
    use assay_core::mcp::tool_decision_truth::{self as tdt, DecisionEvidence};
    use assay_evidence::bundle::BundleWriter;
    use assay_evidence::types::EvidenceEvent;
    use serde_json::{json, Value};

    fn carrier() -> Value {
        let policy: McpPolicy = serde_json::from_value(json!({
            "version": "1",
            "tools": {"allow": ["deploy"], "deny": ["delete_all"]},
            "schemas": {"deploy": {"type": "object", "required": ["env"],
                "properties": {"env": {"enum": ["staging", "prod"]}}}},
            "enforcement": {"unconstrained_tools": "warn"}
        }))
        .unwrap();
        tdt::build_classified_record(
            &policy,
            "deploy",
            &json!({"env": "prod"}),
            0,
            b"project-otel-test-key-v0",
            "fixture-kid-v0",
            "authoritative_boundary",
            "c0",
            "ok",
            "present",
            &DecisionEvidence::default(),
        )
        .unwrap()
    }

    fn ev(type_: &str, payload: Value, seq: u64) -> EvidenceEvent {
        EvidenceEvent::new(type_, "urn:assay:test", "run", seq, payload)
    }

    fn write_bundle(path: &Path, events: Vec<EvidenceEvent>) {
        let f = std::fs::File::create(path).unwrap();
        let mut w = BundleWriter::new(f);
        for e in events {
            w.add_event(e);
        }
        w.finish().unwrap();
    }

    #[test]
    fn projects_a_verified_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("tdt.tar.gz");
        let out = dir.path().join("projection.json");
        let c = carrier();
        let row = tdt::pack_recipe_row(
            &c,
            c["decision_verdict"].as_str().unwrap(),
            "assay://evidence-event/run/0",
        )
        .unwrap();
        write_bundle(
            &bundle,
            vec![
                ev("assay.tool_decision_truth.v0", c, 0),
                ev("assay.tool_decision_truth.recipe_row.v0", row, 1),
            ],
        );

        let code = run_tool_decision_truth(&bundle, Some(&out)).unwrap();
        assert_eq!(code, EXIT_SUCCESS);
        let p: Value = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        let attrs = &p["spans"][0]["attributes"];
        assert_eq!(attrs["assay.claim_class"], json!("derived"));
        assert_eq!(attrs["openinference.span.kind"], json!("TOOL"));
        assert!(attrs["assay.tdt.carrier_content_digest"].is_string());
    }

    #[test]
    fn refuses_to_write_an_unverified_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("tdt.tar.gz");
        let out = dir.path().join("projection.json");
        // A carrier with no recipe row fails semantic verification (rows_present).
        write_bundle(
            &bundle,
            vec![ev("assay.tool_decision_truth.v0", carrier(), 0)],
        );

        let code = run_tool_decision_truth(&bundle, Some(&out)).unwrap();
        assert_eq!(code, EXIT_CONFIG_ERROR);
        // The hard rule: nothing is written when verification fails, not even to --out.
        assert!(!out.exists());
    }
}
