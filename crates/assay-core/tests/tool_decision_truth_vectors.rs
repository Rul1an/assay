//! EXPERIMENTAL conformance vectors for `mcp::tool_decision_truth`.
//!
//! A committed fixture (`tests/fixtures/tool_decision_truth/vectors.json`) of declared-policy + observed
//! decisions with their expected per-decision verdicts and run verdict, plus a couple of pack rows. The
//! guard test reproduces every verdict and re-verifies every pack row FROM THE COMMITTED BYTES, mirroring
//! the private reference-spec's verify-golden discipline. Regenerate the fixture with
//! `UPDATE_TDT_VECTORS=1 cargo test -p assay-core --test tool_decision_truth_vectors`.

use assay_core::mcp::policy::McpPolicy;
use assay_core::mcp::tool_decision_truth as tdt;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/tool_decision_truth/vectors.json")
}

fn declared_policy_json() -> Value {
    json!({
        "version": "1",
        "tools": {"allow": ["read_file", "deploy"], "deny": ["delete_all"]},
        "schemas": {"deploy": {"type": "object", "required": ["env"],
            "properties": {"env": {"enum": ["staging", "prod"]}}}},
        "enforcement": {"unconstrained_tools": "warn"}
    })
}

fn policy() -> McpPolicy {
    serde_json::from_value(declared_policy_json()).unwrap()
}

/// One observed decision spec: (tool, args [None = uncaptured], order, identity_state).
type DecisionSpec = (&'static str, Option<Value>, i64, &'static str);
/// A conformance case: (case id, its observed decisions).
type CaseSpec = (&'static str, Vec<DecisionSpec>);

/// The conformance case specs.
fn case_specs() -> Vec<CaseSpec> {
    vec![
        (
            "match",
            vec![("deploy", Some(json!({"env": "staging"})), 0, "present")],
        ),
        (
            "mismatch_denied_tool",
            vec![("delete_all", Some(json!({})), 0, "present")],
        ),
        (
            "mismatch_arg_enum",
            vec![("deploy", Some(json!({"env": "dev"})), 0, "present")],
        ),
        (
            "incomplete_args_uncaptured",
            vec![("deploy", None, 0, "present")],
        ),
        (
            "incomplete_unconstrained_warn",
            vec![("read_file", Some(json!({"path": "/x"})), 0, "present")],
        ),
        (
            "incomplete_required_missing_identity",
            vec![(
                "deploy",
                Some(json!({"env": "staging"})),
                0,
                "required_missing",
            )],
        ),
        (
            "invalid_identity",
            vec![("deploy", Some(json!({"env": "staging"})), 0, "invalid")],
        ),
        (
            "run_lattice_mismatch",
            vec![
                ("deploy", Some(json!({"env": "staging"})), 0, "present"),
                ("delete_all", Some(json!({})), 1, "present"),
            ],
        ),
    ]
}

fn emit_doc() -> Value {
    let p = policy();
    let mut cases = Vec::new();
    for (id, decisions) in case_specs() {
        let mut observed = Vec::new();
        let mut verdicts: Vec<&str> = Vec::new();
        let mut orders: Vec<i64> = Vec::new();
        for (tool, args, order, id_state) in &decisions {
            verdicts.push(tdt::decision_verdict(&p, tool, args.as_ref(), id_state));
            orders.push(*order);
            observed.push(json!({
                "tool_name": tool,
                "args": args,
                "order": order,
                "identity_state": id_state,
            }));
        }
        let run = tdt::run_verdict(&verdicts, &orders);
        cases.push(json!({
            "id": id,
            "observed": observed,
            "expected": {"decisions": verdicts, "run_verdict": run},
        }));
    }
    let mut pack_rows = Vec::new();
    for (oid, dpd, verdict, reference) in [
        (
            "sha256:obs-1",
            "sha256:decl-1",
            "match",
            "audit://decision/c1",
        ),
        (
            "sha256:obs-2",
            "sha256:decl-2",
            "mismatch",
            "audit://decision/c2",
        ),
    ] {
        let row = tdt::pack_recipe_row(oid, dpd, verdict, reference).unwrap();
        pack_rows.push(json!({
            "observed_input_digest": oid,
            "declared_policy_digest": dpd,
            "run_verdict": verdict,
            "ref": reference,
            "row": row,
        }));
    }
    json!({
        "schema": "assay.tool_decision_truth.vectors.v0",
        "declared_policy": declared_policy_json(),
        "cases": cases,
        "pack_rows": pack_rows,
    })
}

#[test]
fn vectors_in_sync_and_reproduce_from_bytes() {
    let fresh = emit_doc();
    let path = fixture_path();
    if std::env::var("UPDATE_TDT_VECTORS").is_ok() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            format!("{}\n", serde_json::to_string_pretty(&fresh).unwrap()),
        )
        .unwrap();
    }
    let committed: Value = serde_json::from_str(
        &fs::read_to_string(&path)
            .expect("vectors.json present (regenerate with UPDATE_TDT_VECTORS=1)"),
    )
    .unwrap();

    // Sync-guard: the committed fixture must not drift from the current code.
    assert_eq!(
        committed, fresh,
        "vectors.json drifted from emit; regenerate with UPDATE_TDT_VECTORS=1"
    );

    // Reproduce-from-bytes: recompute every verdict and re-verify every pack row from the committed bytes.
    let p: McpPolicy = serde_json::from_value(committed["declared_policy"].clone()).unwrap();
    for case in committed["cases"].as_array().unwrap() {
        let mut verdicts: Vec<&str> = Vec::new();
        let mut orders: Vec<i64> = Vec::new();
        for d in case["observed"].as_array().unwrap() {
            let tool = d["tool_name"].as_str().unwrap();
            let args = if d["args"].is_null() {
                None
            } else {
                Some(&d["args"])
            };
            let id_state = d["identity_state"].as_str().unwrap();
            verdicts.push(tdt::decision_verdict(&p, tool, args, id_state));
            orders.push(d["order"].as_i64().unwrap());
        }
        let expected: Vec<&str> = case["expected"]["decisions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap())
            .collect();
        assert_eq!(
            verdicts, expected,
            "per-decision verdicts for case {}",
            case["id"]
        );
        assert_eq!(
            tdt::run_verdict(&verdicts, &orders),
            case["expected"]["run_verdict"].as_str().unwrap(),
            "run verdict for case {}",
            case["id"]
        );
    }
    for row in committed["pack_rows"].as_array().unwrap() {
        assert!(
            tdt::verify_recipe_row(
                &row["row"],
                row["observed_input_digest"].as_str().unwrap(),
                row["declared_policy_digest"].as_str().unwrap(),
                row["run_verdict"].as_str().unwrap(),
            ),
            "pack row did not reproduce from bytes"
        );
    }
}
