//! P60a guard: the MCP tool-manifest drift reference fixtures must reproduce their committed
//! canonical digests from the documented projection (docs/reference/mcp-manifest-drift.md) via the
//! same JCS the producer (P60b) and consumer (P60c) will use, and the documented coverage-rule table
//! must be executable. There is no producer/consumer yet; this keeps the vectors honest and proves
//! the canonicalization and the verdict rules are sound. Manifest drift is canonical-digest evidence,
//! not maliciousness evidence.

use assay_core::mcp::jcs;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

fn fx(name: &str) -> Value {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mcp_manifest_drift")
        .join(name);
    serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap()
}

fn digest_of(subject: &Value) -> String {
    let bytes = jcs::to_vec(subject).expect("jcs");
    format!("sha256:{}", hex::encode(Sha256::digest(&bytes)))
}

/// A per-tool projection hashes directly: the fixture tool objects are already in projection shape
/// (name, description, input_schema, output_schema, annotations).
fn tool_digest(tool: &Value) -> String {
    digest_of(tool)
}

/// P60d-v2 per-field digest: projection id + field name inside the hashed preimage.
fn field_digest(field: &str, value: &Value) -> String {
    digest_of(&json!({"projection": "assay.mcp_tool_field.v0", "field": field, "value": value}))
}

#[test]
fn field_digests_anchor_recomputes_from_committed_bytes() {
    // The committed per-field anchor recomputes via the same JCS the producer uses (cross-impl), and
    // it is independent of tool_digest/manifest_digest (which the other tests pin unchanged).
    let ex = fx("canonicalization_example.json");
    let proj = &ex["per_tool"]["projection"];
    let expected = &ex["per_tool"]["expected_field_digests"];
    for field in [
        "description",
        "input_schema",
        "output_schema",
        "annotations",
    ] {
        assert_eq!(
            field_digest(field, &proj[field]),
            expected[field].as_str().unwrap(),
            "{field} field_digest must recompute from the committed projection"
        );
    }
    // Domain separation: a null output_schema and a null annotations must not collide.
    assert_ne!(expected["output_schema"], expected["annotations"]);
}

/// Build the manifest projection with the projection id INSIDE the hashed preimage, sorted by name
/// then tool_digest, and hash it. This is the value the v0 Plimsoll gate compares against baseline.
fn manifest_digest(tools: &[Value]) -> String {
    let mut entries: Vec<Value> = tools
        .iter()
        .map(|t| json!({"name": t["name"], "tool_digest": tool_digest(t)}))
        .collect();
    entries.sort_by(|a, b| {
        (
            a["name"].as_str().unwrap(),
            a["tool_digest"].as_str().unwrap(),
        )
            .cmp(&(
                b["name"].as_str().unwrap(),
                b["tool_digest"].as_str().unwrap(),
            ))
    });
    digest_of(&json!({
        "projection": "assay.mcp_manifest_projection.v0",
        "tools": entries,
    }))
}

fn has_duplicate_names(tools: &[Value]) -> bool {
    let mut seen: HashMap<&str, u32> = HashMap::new();
    for t in tools {
        *seen.entry(t["name"].as_str().unwrap()).or_insert(0) += 1;
    }
    seen.values().any(|&n| n > 1)
}

/// Reference verifier for the documented v0 coverage-rule table. The production gate is P60c; this
/// proves the spec's verdict column is executable and the fixtures agree with it.
fn verdict(baseline_digest: &str, observed: &Value) -> &'static str {
    let observed_flag = observed["tools_list_observed"].as_bool().unwrap();
    if !observed_flag {
        return "inconclusive_manifest_not_observed";
    }
    let tools: Vec<Value> = observed["tools"].as_array().unwrap().clone();
    if has_duplicate_names(&tools) {
        return "manifest_observation_ambiguous";
    }
    let complete = observed["tools_list_complete"].as_str().unwrap();
    if complete == "partial" {
        return "inconclusive_manifest_partial_observation";
    }
    let drift = manifest_digest(&tools) != baseline_digest;
    if drift {
        // Drift is observed even under `unknown` completeness.
        return "pending_tool_manifest_review";
    }
    // Digest matches: only `complete` completeness is fully clean; `unknown` carries a coverage warning.
    if complete == "unknown" {
        "no_finding_with_coverage_warning"
    } else {
        "no_finding"
    }
}

#[test]
fn canonicalization_example_recomputes_from_committed_bytes() {
    // A third implementation must reproduce these digests from the documented projection alone.
    let ex = fx("canonicalization_example.json");
    assert_eq!(
        tool_digest(&ex["per_tool"]["projection"]),
        ex["per_tool"]["expected_tool_digest"].as_str().unwrap(),
        "per-tool projection must recompute to its committed tool_digest via canonical JCS"
    );
    let tools: Vec<Value> = ex["manifest"]["tools"].as_array().unwrap().clone();
    assert_eq!(
        manifest_digest(&tools),
        ex["manifest"]["expected_manifest_digest"].as_str().unwrap(),
        "manifest projection must recompute to its committed manifest_digest via canonical JCS"
    );
}

#[test]
fn manifest_digest_is_order_independent() {
    // The manifest digest must be stable under tool reordering (entries are sorted in the preimage).
    let ex = fx("canonicalization_example.json");
    let mut tools: Vec<Value> = ex["manifest"]["tools"].as_array().unwrap().clone();
    let forward = manifest_digest(&tools);
    tools.reverse();
    assert_eq!(
        forward,
        manifest_digest(&tools),
        "manifest digest must not depend on tool order"
    );
}

#[test]
fn projection_id_is_inside_the_preimage() {
    // Changing the projection id must change the manifest digest (it is hashed, not metadata-only).
    let ex = fx("canonicalization_example.json");
    let mut entries: Vec<Value> = ex["manifest"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| json!({"name": t["name"], "tool_digest": tool_digest(t)}))
        .collect();
    entries.sort_by(|a, b| {
        (
            a["name"].as_str().unwrap(),
            a["tool_digest"].as_str().unwrap(),
        )
            .cmp(&(
                b["name"].as_str().unwrap(),
                b["tool_digest"].as_str().unwrap(),
            ))
    });
    let canonical =
        digest_of(&json!({"projection": "assay.mcp_manifest_projection.v0", "tools": entries}));
    let tampered = digest_of(&json!({"projection": "assay.other_projection.v0", "tools": entries}));
    assert_ne!(
        canonical, tampered,
        "projection id must be part of the hashed preimage"
    );
}

#[test]
fn verdict_corpus_matches_expected() {
    let corpus = fx("verdict_corpus.json");
    let mut failures = Vec::new();
    for case in corpus["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let baseline = case["baseline_manifest_digest"].as_str().unwrap();
        let got = verdict(baseline, &case["observed"]);
        let expected = case["expected_verdict"].as_str().unwrap();
        if got != expected {
            failures.push(format!("{id}: expected {expected}, got {got}"));
        }
    }
    assert!(failures.is_empty(), "verdict mismatches: {failures:?}");
}

#[test]
fn pinned_verdict_vocabulary() {
    // No case may carry a verdict outside the documented v0 set.
    let known = [
        "no_finding",
        "no_finding_with_coverage_warning",
        "pending_tool_manifest_review",
        "inconclusive_manifest_not_observed",
        "inconclusive_manifest_partial_observation",
        "manifest_observation_ambiguous",
    ];
    let corpus = fx("verdict_corpus.json");
    for case in corpus["cases"].as_array().unwrap() {
        let v = case["expected_verdict"].as_str().unwrap();
        assert!(
            known.contains(&v),
            "{}: unknown verdict {v}",
            case["id"].as_str().unwrap()
        );
    }
}
