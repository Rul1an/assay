//! P60d-a guard: granular per-tool MCP manifest drift (Option A — presence + per-tool digest, no
//! producer change). Spec: docs/reference/mcp-manifest-drift.md. There is no consumer code yet (that
//! is P60d-b in Plimsoll); this proves the declared baseline is self-consistent (its manifest_digest
//! recomputes via the same JCS the producer uses, anchored to the committed P60a value) and that the
//! documented per-tool finding matrix + validity checks are executable. P60d explains which tool digest
//! drifted; it does not explain which field changed or whether the change is malicious.

use assay_core::mcp::jcs;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::PathBuf;

fn fx(name: &str) -> Value {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mcp_manifest_drift")
        .join(name);
    serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap()
}

/// Recompute a manifest_digest from per-tool {name, tool_digest} entries via the documented JCS
/// canonicalization (projection id inside the hashed preimage, entries sorted by name then digest).
fn recompute_manifest_digest(tools: &[Value]) -> String {
    let mut entries: Vec<(String, String)> = tools
        .iter()
        .map(|t| {
            (
                t["name"].as_str().unwrap().to_string(),
                t["tool_digest"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    entries.sort();
    let arr: Vec<Value> = entries
        .into_iter()
        .map(|(n, d)| json!({"name": n, "tool_digest": d}))
        .collect();
    let bytes = jcs::to_vec(&json!({
        "projection": "assay.mcp_manifest_projection.v0",
        "tools": arr,
    }))
    .unwrap();
    format!("sha256:{}", hex::encode(Sha256::digest(&bytes)))
}

fn dup_names(tools: &[Value]) -> bool {
    let mut seen = std::collections::HashSet::new();
    tools
        .iter()
        .any(|t| !seen.insert(t["name"].as_str().unwrap()))
}

/// Reference verifier for the documented P60d-a granular-diff matrix + validity checks. The production
/// consumer is P60d-b (Plimsoll); this proves the spec's columns are executable.
fn diff(observed: &Value, declared: &Value) -> (Vec<(String, String)>, Vec<String>) {
    let dtools = declared["tools"].as_array().unwrap();

    // Baseline validity first — never diff against an invalid/inconsistent baseline.
    if recompute_manifest_digest(dtools) != declared["manifest_digest"].as_str().unwrap() {
        return (vec![], vec!["declared_manifest_digest_mismatch".into()]);
    }
    if dup_names(dtools) {
        return (vec![], vec!["declared_mcp_manifest_ambiguous".into()]);
    }
    if observed["server"]["id"] != declared["server"]["id"] {
        return (vec![], vec!["mcp_manifest_server_mismatch".into()]);
    }
    if observed["observed"]["canonicalization"] != declared["canonicalization"] {
        return (
            vec![],
            vec!["mcp_manifest_canonicalization_mismatch".into()],
        );
    }

    // Observed validity.
    let status = observed["status"].as_str().unwrap();
    if status == "not_observed" {
        return (vec![], vec!["inconclusive_manifest_not_observed".into()]);
    }
    let otools = observed["observed"]["tool_digests"].as_array().unwrap();
    if status == "ambiguous" || dup_names(otools) {
        return (vec![], vec!["mcp_manifest_observation_ambiguous".into()]);
    }

    // Per-tool diff.
    let complete = observed["observed"]["tools_list_complete"]
        .as_str()
        .unwrap()
        == "complete";
    let obs: HashMap<&str, (&str, bool)> = otools
        .iter()
        .map(|t| {
            (
                t["name"].as_str().unwrap(),
                (
                    t["tool_digest"].as_str().unwrap(),
                    t["privileged"].as_bool().unwrap(),
                ),
            )
        })
        .collect();
    let dec: HashMap<&str, (&str, bool)> = dtools
        .iter()
        .map(|t| {
            (
                t["name"].as_str().unwrap(),
                (
                    t["tool_digest"].as_str().unwrap(),
                    t["privileged"].as_bool().unwrap(),
                ),
            )
        })
        .collect();
    let names: BTreeSet<&str> = obs.keys().chain(dec.keys()).copied().collect();

    let mut findings = Vec::new();
    let mut inconclusive = Vec::new();
    let mut removal_inconclusive = false;
    for name in names {
        match (obs.get(name), dec.get(name)) {
            (Some((_, opriv)), None) => {
                // observed-only -> added
                let code = if *opriv {
                    "mcp_new_privileged_tool"
                } else {
                    "mcp_tool_added"
                };
                findings.push((name.to_string(), code.to_string()));
            }
            (None, Some((_, dpriv))) => {
                // declared-only -> removed, but only assertable under complete observation
                if complete {
                    let code = if *dpriv {
                        "mcp_privileged_tool_removed"
                    } else {
                        "mcp_tool_removed"
                    };
                    findings.push((name.to_string(), code.to_string()));
                } else if !removal_inconclusive {
                    inconclusive.push("inconclusive_manifest_partial_observation".to_string());
                    removal_inconclusive = true;
                }
            }
            (Some((od, opriv)), Some((dd, dpriv))) => {
                if od != dd {
                    let code = if *opriv || *dpriv {
                        "mcp_privileged_tool_changed"
                    } else {
                        "mcp_tool_changed"
                    };
                    findings.push((name.to_string(), code.to_string()));
                }
            }
            (None, None) => unreachable!(),
        }
    }
    (findings, inconclusive)
}

#[test]
fn declared_baseline_is_self_consistent_and_anchored_to_p60a() {
    let base = fx("declared_per_tool_baseline.json");
    assert_eq!(base["schema"], "assay.declared_mcp_manifest.v0");
    // Self-consistency: manifest_digest recomputes from the per-tool entries.
    assert_eq!(
        recompute_manifest_digest(base["tools"].as_array().unwrap()),
        base["manifest_digest"].as_str().unwrap(),
        "declared baseline manifest_digest must recompute from its tools"
    );
    // Anchor: the same canonical tools yield the committed P60a manifest_digest.
    let p60a = fx("canonicalization_example.json");
    assert_eq!(
        base["manifest_digest"].as_str().unwrap(),
        p60a["manifest"]["expected_manifest_digest"]
            .as_str()
            .unwrap(),
        "a clean per-tool baseline equals the committed P60a manifest_digest"
    );
}

#[test]
fn declared_baseline_carries_field_digests_consistent_with_the_anchor() {
    // P60d-v2: the declared baseline gains optional field_digests (additive — the v1 manifest_digest
    // self-consistency test above still passes, since field_digests are outside that preimage).
    let base = fx("declared_per_tool_baseline.json");
    let anchor = fx("canonicalization_example.json");
    for t in base["tools"].as_array().unwrap() {
        let fd = &t["field_digests"];
        assert!(fd.is_object(), "each declared tool carries field_digests");
        for field in [
            "description",
            "input_schema",
            "output_schema",
            "annotations",
        ] {
            assert!(fd[field].is_string(), "{field} present");
        }
    }
    // The privileged deploy tool's per-field digests equal the committed anchor (same canonical tool).
    let deploy = base["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "github.add_deploy_key")
        .unwrap();
    assert_eq!(
        deploy["field_digests"], anchor["per_tool"]["expected_field_digests"],
        "baseline field_digests must match the committed anchor for the same tool"
    );
}

#[test]
fn granular_diff_corpus_matches_expected() {
    let corpus = fx("granular_diff_cases.json");
    let mut failures = Vec::new();
    for case in corpus["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let (findings, inconclusive) = diff(&case["observed"], &case["declared"]);

        let mut got: Vec<String> = findings.iter().map(|(n, c)| format!("{n}:{c}")).collect();
        got.sort();
        let mut want: Vec<String> = case["expected"]["findings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| {
                format!(
                    "{}:{}",
                    f["name"].as_str().unwrap(),
                    f["reason_code"].as_str().unwrap()
                )
            })
            .collect();
        want.sort();
        if got != want {
            failures.push(format!("{id}: findings expected {want:?}, got {got:?}"));
        }

        let mut got_inc = inconclusive.clone();
        got_inc.sort();
        let mut want_inc: Vec<String> = case["expected"]["inconclusive"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap().to_string())
            .collect();
        want_inc.sort();
        if got_inc != want_inc {
            failures.push(format!(
                "{id}: inconclusive expected {want_inc:?}, got {got_inc:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "granular diff mismatches: {failures:#?}"
    );
}

#[test]
fn pinned_p60d_vocabulary() {
    let known_findings = [
        "mcp_tool_added",
        "mcp_new_privileged_tool",
        "mcp_tool_removed",
        "mcp_privileged_tool_removed",
        "mcp_tool_changed",
        "mcp_privileged_tool_changed",
    ];
    let known_inconclusive = [
        "declared_manifest_digest_mismatch",
        "declared_mcp_manifest_ambiguous",
        "mcp_manifest_server_mismatch",
        "mcp_manifest_canonicalization_mismatch",
        "inconclusive_manifest_not_observed",
        "mcp_manifest_observation_ambiguous",
        "inconclusive_manifest_partial_observation",
    ];
    let corpus = fx("granular_diff_cases.json");
    for case in corpus["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        for f in case["expected"]["findings"].as_array().unwrap() {
            let c = f["reason_code"].as_str().unwrap();
            assert!(
                known_findings.contains(&c),
                "{id}: unknown finding code {c}"
            );
        }
        for c in case["expected"]["inconclusive"].as_array().unwrap() {
            let c = c.as_str().unwrap();
            assert!(
                known_inconclusive.contains(&c),
                "{id}: unknown inconclusive code {c}"
            );
        }
    }
}
