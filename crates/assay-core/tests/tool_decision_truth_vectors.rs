//! EXPERIMENTAL conformance vectors for `mcp::tool_decision_truth`.
//!
//! A committed fixture (`tests/fixtures/tool_decision_truth/vectors.json`) of declared policies + observed
//! decisions with their expected per-decision and run verdicts, REAL emitted carriers (so the observed-
//! input digest layer is pinned), positive pack rows built from those carriers, and negative pack rows
//! that must fail closed. The guard test reproduces every verdict and re-verifies every row FROM THE
//! COMMITTED BYTES, mirroring the private reference-spec's verify-golden discipline. Regenerate with
//! `UPDATE_TDT_VECTORS=1 cargo test -p assay-core --test tool_decision_truth_vectors`.

use assay_core::mcp::policy::McpPolicy;
use assay_core::mcp::tool_decision_truth as tdt;
use assay_core::mcp::tool_decision_truth::DecisionEvidence;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

// A fixed test key/key_id: real carriers are keyed, but the key never enters the fixture (only key_id).
const TEST_KEY: &[u8] = b"tool-decision-truth-vectors-key-v0";
const TEST_KID: &str = "test-key-v0";

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/tool_decision_truth/vectors.json")
}

// ── declared policies ─────────────────────────────────────────────────────────────────────────────
// Several policies so the vectors exercise pattern deny, an unsupported declared surface, and a malformed
// schema, not just the narrow allow/deny + flat-schema happy path.

fn policies() -> Vec<(&'static str, Value)> {
    vec![
        (
            "base",
            json!({
                "version": "1",
                "tools": {"allow": ["read_file", "deploy"], "deny": ["delete_all"]},
                "schemas": {"deploy": {"type": "object", "required": ["env"],
                    "properties": {"env": {"enum": ["staging", "prod"]}}}},
                "enforcement": {"unconstrained_tools": "warn"}
            }),
        ),
        (
            "pattern_deny",
            json!({
                "version": "1",
                "tools": {"deny": ["delete_*"]},
                "enforcement": {"unconstrained_tools": "allow"}
            }),
        ),
        (
            "approval",
            json!({
                "version": "1",
                "tools": {"allow": ["pay"], "approval_required": ["pay"]},
                "enforcement": {"unconstrained_tools": "allow"}
            }),
        ),
        (
            "malformed_schema",
            json!({
                "version": "1",
                "tools": {"allow": ["t"]},
                "schemas": {"t": {"$ref": "#/$defs/missing"}},
                "enforcement": {"unconstrained_tools": "allow"}
            }),
        ),
    ]
}

fn policy_json(name: &str) -> Value {
    policies()
        .into_iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("unknown policy {name}"))
        .1
}

fn policy(name: &str) -> McpPolicy {
    serde_json::from_value(policy_json(name)).unwrap()
}

fn evidence_from(v: Option<&Value>) -> DecisionEvidence {
    let Some(v) = v else {
        return DecisionEvidence::default();
    };
    DecisionEvidence {
        tool_classes: v.get("tool_classes").and_then(|x| x.as_array()).map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(String::from))
                .collect()
        }),
        approval_obtained: v.get("approval_obtained").and_then(|x| x.as_bool()),
        scope_satisfied: v.get("scope_satisfied").and_then(|x| x.as_bool()),
        redaction_applied: v.get("redaction_applied").and_then(|x| x.as_bool()),
    }
}

// ── verdict case specs ────────────────────────────────────────────────────────────────────────────

/// One observed decision: (tool, args [None = uncaptured], order, identity_state, evidence [None = none]).
type DecisionSpec = (
    &'static str,
    Option<Value>,
    i64,
    &'static str,
    Option<Value>,
);
/// A conformance case: (id, declared-policy name, observed decisions).
type CaseSpec = (&'static str, &'static str, Vec<DecisionSpec>);

fn case_specs() -> Vec<CaseSpec> {
    vec![
        (
            "match",
            "base",
            vec![(
                "deploy",
                Some(json!({"env": "staging"})),
                0,
                "present",
                None,
            )],
        ),
        (
            "mismatch_denied_tool",
            "base",
            vec![("delete_all", Some(json!({})), 0, "present", None)],
        ),
        (
            "mismatch_arg_enum",
            "base",
            vec![("deploy", Some(json!({"env": "dev"})), 0, "present", None)],
        ),
        (
            "incomplete_args_uncaptured",
            "base",
            vec![("deploy", None, 0, "present", None)],
        ),
        (
            "incomplete_unconstrained_warn",
            "base",
            vec![("read_file", Some(json!({"path": "/x"})), 0, "present", None)],
        ),
        (
            "incomplete_required_missing_identity",
            "base",
            vec![(
                "deploy",
                Some(json!({"env": "staging"})),
                0,
                "required_missing",
                None,
            )],
        ),
        (
            "invalid_identity",
            "base",
            vec![(
                "deploy",
                Some(json!({"env": "staging"})),
                0,
                "invalid",
                None,
            )],
        ),
        (
            "absent_identity_match",
            "base",
            vec![("deploy", Some(json!({"env": "prod"})), 0, "absent", None)],
        ),
        (
            "run_lattice_mismatch",
            "base",
            vec![
                (
                    "deploy",
                    Some(json!({"env": "staging"})),
                    0,
                    "present",
                    None,
                ),
                ("delete_all", Some(json!({})), 1, "present", None),
            ],
        ),
        (
            "invalid_duplicate_order",
            "base",
            vec![
                (
                    "deploy",
                    Some(json!({"env": "staging"})),
                    0,
                    "present",
                    None,
                ),
                ("read_file", Some(json!({"path": "/x"})), 0, "present", None),
            ],
        ),
        // pattern deny: a literal delete_* deny blocks delete_all (exact equality would have missed it).
        (
            "mismatch_pattern_deny",
            "pattern_deny",
            vec![("delete_all", Some(json!({})), 0, "present", None)],
        ),
        // unsupported declared surface with no evidence -> incomplete, never a silent match.
        (
            "incomplete_approval_no_evidence",
            "approval",
            vec![("pay", Some(json!({"amount": 10})), 0, "present", None)],
        ),
        // ... and it resolves once the approval evidence is supplied.
        (
            "match_approval_evidence",
            "approval",
            vec![(
                "pay",
                Some(json!({"amount": 10})),
                0,
                "present",
                Some(json!({"approval_obtained": true})),
            )],
        ),
        (
            "mismatch_approval_denied",
            "approval",
            vec![(
                "pay",
                Some(json!({"amount": 10})),
                0,
                "present",
                Some(json!({"approval_obtained": false})),
            )],
        ),
        // a malformed declared schema is the declaration's fault -> invalid (and must not panic).
        (
            "invalid_malformed_schema",
            "malformed_schema",
            vec![("t", Some(json!({})), 0, "present", None)],
        ),
    ]
}

/// Direct run-lattice units: arity mismatch and duplicate order both force `invalid`.
fn run_verdict_unit_specs() -> Vec<(&'static str, Vec<&'static str>, Vec<i64>)> {
    vec![
        ("arity_more_orders", vec!["match"], vec![0, 1]),
        ("arity_more_verdicts", vec!["match", "match"], vec![0]),
        ("duplicate_order", vec!["match", "match"], vec![0, 0]),
        ("clean_run", vec!["match", "incomplete"], vec![0, 1]),
    ]
}

// ── real carriers ─────────────────────────────────────────────────────────────────────────────────

/// A carrier build spec: (id, policy, tool, args, order, identity_state, evidence).
type CarrierSpec = (
    &'static str,
    &'static str,
    &'static str,
    Value,
    i64,
    &'static str,
    Option<Value>,
);

fn carrier_specs() -> Vec<CarrierSpec> {
    vec![
        (
            "deploy_match",
            "base",
            "deploy",
            json!({"env": "prod"}),
            0,
            "present",
            None,
        ),
        (
            "delete_mismatch",
            "base",
            "delete_all",
            json!({}),
            1,
            "present",
            None,
        ),
        // carries a secret-named arg: the raw value must never appear, only its dropped-then-keyed digest.
        (
            "secret_args",
            "base",
            "read_file",
            json!({"path": "/x", "token": "SUPER-SECRET"}),
            2,
            "present",
            None,
        ),
    ]
}

fn build_carrier(spec: &CarrierSpec) -> Value {
    let (id, policy_name, tool, args, order, identity, evidence) = spec;
    tdt::build_classified_record(
        &policy(policy_name),
        tool,
        args,
        *order,
        TEST_KEY,
        TEST_KID,
        "authoritative_boundary",
        id,
        "ok",
        identity,
        &evidence_from(evidence.as_ref()),
    )
    .unwrap_or_else(|| panic!("carrier {id} did not build"))
}

/// Positive pack rows: (id, carrier id, run_verdict, ref). The run verdict is >= the carrier's decision.
fn pack_row_specs() -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
    vec![
        ("match_row", "deploy_match", "match", "audit://decision/c0"),
        (
            "mismatch_row",
            "delete_mismatch",
            "mismatch",
            "audit://decision/c1",
        ),
    ]
}

// ── emit ────────────────────────────────────────────────────────────────────────────────────────

fn emit_doc() -> Value {
    // policies
    let mut policies_doc = serde_json::Map::new();
    for (name, json) in policies() {
        policies_doc.insert(name.to_string(), json);
    }

    // verdict cases
    let mut cases = Vec::new();
    for (id, policy_name, decisions) in case_specs() {
        let p = policy(policy_name);
        let mut observed = Vec::new();
        let mut verdicts: Vec<&str> = Vec::new();
        let mut orders: Vec<i64> = Vec::new();
        for (tool, args, order, id_state, evidence) in &decisions {
            let ev = evidence_from(evidence.as_ref());
            verdicts.push(tdt::decision_verdict(
                &p,
                tool,
                args.as_ref(),
                id_state,
                &ev,
            ));
            orders.push(*order);
            observed.push(json!({
                "tool_name": tool,
                "args": args,
                "order": order,
                "identity_state": id_state,
                "evidence": evidence,
            }));
        }
        let run = tdt::run_verdict(&verdicts, &orders);
        cases.push(json!({
            "id": id,
            "policy": policy_name,
            "observed": observed,
            "expected": {"decisions": verdicts, "run_verdict": run},
        }));
    }

    // run-lattice units
    let mut run_units = Vec::new();
    for (id, verdicts, orders) in run_verdict_unit_specs() {
        let expected = tdt::run_verdict(&verdicts, &orders);
        run_units
            .push(json!({"id": id, "verdicts": verdicts, "orders": orders, "expected": expected}));
    }

    // real carriers
    let mut carriers = Vec::new();
    for spec in carrier_specs() {
        let carrier = build_carrier(&spec);
        carriers.push(json!({"id": spec.0, "policy": spec.1, "carrier": carrier}));
    }
    let carrier_of = |id: &str| -> Value {
        carriers
            .iter()
            .find(|c| c["id"] == id)
            .map(|c| c["carrier"].clone())
            .unwrap_or_else(|| panic!("carrier {id} not emitted"))
    };

    // positive pack rows from real carriers
    let mut pack_rows = Vec::new();
    for (id, carrier_id, run_verdict, reference) in pack_row_specs() {
        let carrier = carrier_of(carrier_id);
        let row = tdt::pack_recipe_row(&carrier, run_verdict, reference)
            .unwrap_or_else(|| panic!("pack row {id} did not build"));
        pack_rows.push(json!({
            "id": id,
            "carrier_id": carrier_id,
            "run_verdict": run_verdict,
            "row": row,
        }));
    }

    // negative pack rows: each must fail verify_recipe_row against the cited carrier.
    let good =
        tdt::pack_recipe_row(&carrier_of("deploy_match"), "match", "audit://decision/c0").unwrap();
    let mut negatives = Vec::new();

    // (a) tampered run_verdict: the bound verdict was "match"; the row now claims "incomplete".
    let mut tampered_verdict = good.clone();
    tampered_verdict["run_verdict"] = json!("incomplete");
    negatives.push(json!({
        "id": "tampered_run_verdict", "carrier_id": "deploy_match",
        "run_verdict": "incomplete", "row": tampered_verdict,
    }));

    // (b) foreign recipe.
    let mut foreign_recipe = good.clone();
    foreign_recipe["recipe"] = json!("other.recipe.v0");
    negatives.push(json!({
        "id": "foreign_recipe", "carrier_id": "deploy_match",
        "run_verdict": "match", "row": foreign_recipe,
    }));

    // (c) foreign canonicalization in the envelope.
    let mut foreign_canon = good.clone();
    foreign_canon["evidence_ref"]["canonicalization"] = json!("cbor-deterministic-v1");
    negatives.push(json!({
        "id": "foreign_canonicalization", "carrier_id": "deploy_match",
        "run_verdict": "match", "row": foreign_canon,
    }));

    // (d) malformed citation digest (not sha256:<64hex>).
    let mut bad_digest = good.clone();
    bad_digest["evidence_ref"]["digest"] = json!("sha256:short");
    negatives.push(json!({
        "id": "malformed_citation_digest", "carrier_id": "deploy_match",
        "run_verdict": "match", "row": bad_digest,
    }));

    json!({
        "schema": "assay.tool_decision_truth.vectors.v0",
        "policies": Value::Object(policies_doc),
        "cases": cases,
        "run_verdict_units": run_units,
        "carriers": carriers,
        "pack_rows": pack_rows,
        "negative_pack_rows": negatives,
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

    // Sync-guard: the committed fixture must not drift from the current code (this is what pins the
    // observed-input digest layer, since the carriers are emitted from the real keyed digests).
    assert_eq!(
        committed, fresh,
        "vectors.json drifted from emit; regenerate with UPDATE_TDT_VECTORS=1"
    );

    // Reproduce verdicts from the committed bytes.
    for case in committed["cases"].as_array().unwrap() {
        let p: McpPolicy =
            serde_json::from_value(policy_json(case["policy"].as_str().unwrap())).unwrap();
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
            let ev = evidence_from(d.get("evidence"));
            verdicts.push(tdt::decision_verdict(&p, tool, args, id_state, &ev));
            orders.push(
                d["order"]
                    .as_i64()
                    .expect("vectors: decision order must be an integer"),
            );
        }
        let expected: Vec<&str> = case["expected"]["decisions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap())
            .collect();
        assert_eq!(verdicts, expected, "decisions for case {}", case["id"]);
        assert_eq!(
            tdt::run_verdict(&verdicts, &orders),
            case["expected"]["run_verdict"].as_str().unwrap(),
            "run verdict for case {}",
            case["id"]
        );
    }

    // Reproduce the run-lattice units.
    for unit in committed["run_verdict_units"].as_array().unwrap() {
        let verdicts: Vec<&str> = unit["verdicts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap())
            .collect();
        let orders: Vec<i64> = unit["orders"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_i64().unwrap())
            .collect();
        assert_eq!(
            tdt::run_verdict(&verdicts, &orders),
            unit["expected"].as_str().unwrap(),
            "run_verdict unit {}",
            unit["id"]
        );
    }

    // The observed-input digest layer: every carrier carries a keyed args_digest (framed with its key_id)
    // and never the raw arguments.
    let mut carrier_by_id = std::collections::HashMap::new();
    for c in committed["carriers"].as_array().unwrap() {
        let carrier = &c["carrier"];
        carrier_by_id.insert(c["id"].as_str().unwrap().to_string(), carrier.clone());
        assert!(
            carrier.get("args").is_none() && carrier.get("arguments").is_none(),
            "carrier {} must not carry raw args",
            c["id"]
        );
        assert!(
            carrier["args_digest"]
                .as_str()
                .unwrap()
                .starts_with(&format!("hmac-sha256:{TEST_KID}:")),
            "carrier {} args_digest must be keyed and framed by key_id",
            c["id"]
        );
        assert_eq!(carrier["key_id"], json!(TEST_KID));
    }
    // The secret value never appears anywhere in the secret-args carrier.
    let secret_carrier = serde_json::to_string(&carrier_by_id["secret_args"]).unwrap();
    assert!(!secret_carrier.contains("SUPER-SECRET"));
    // Secret-drop: a secret-named arg does not affect the keyed digest.
    assert_eq!(
        tdt::args_digest(
            &json!({"path": "/x", "token": "SUPER-SECRET"}),
            TEST_KEY,
            TEST_KID
        ),
        tdt::args_digest(&json!({"path": "/x"}), TEST_KEY, TEST_KID),
        "secret-named keys must drop out of the args_digest"
    );

    // Positive pack rows reproduce: each verifies against the real carrier it cites.
    for row in committed["pack_rows"].as_array().unwrap() {
        let carrier = &carrier_by_id[row["carrier_id"].as_str().unwrap()];
        assert!(
            tdt::verify_recipe_row(&row["row"], carrier, row["run_verdict"].as_str().unwrap()),
            "positive pack row {} did not reproduce",
            row["id"]
        );
    }

    // Negative pack rows fail closed: each must NOT verify.
    for row in committed["negative_pack_rows"].as_array().unwrap() {
        let carrier = &carrier_by_id[row["carrier_id"].as_str().unwrap()];
        assert!(
            !tdt::verify_recipe_row(&row["row"], carrier, row["run_verdict"].as_str().unwrap()),
            "negative pack row {} must fail verification",
            row["id"]
        );
    }
}
