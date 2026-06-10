//! P60b: build the observed MCP tool-manifest record (`assay.mcp_manifest_observed.v0`) from an
//! observed `tools/list`. Spec + canonicalization + coverage rules: docs/reference/mcp-manifest-drift.md.
//!
//! This is a PRODUCER ONLY. It does not decide whether drift matters, does not compare against a
//! baseline, and emits no findings — that is the consumer's job (P60c). The load-bearing rules it
//! enforces by construction:
//!
//! - **Manifest drift is canonical-digest evidence, not maliciousness evidence.** This module computes
//!   digests; it never judges a change.
//! - **Canonicalization is exactly P60a.** The per-tool projection is `{name, description,
//!   input_schema, output_schema, annotations}` and the manifest projection carries its projection id
//!   INSIDE the hashed preimage, both over JCS (RFC 8785) — the same bytes the P60a guard fixtures
//!   were committed against, so this producer reproduces those committed digests.
//! - **`privileged` is classifier-derived, never server annotations.** It is taken from the P57c
//!   classifier keyed on the tool name; the server's own annotations are carried into the digest but
//!   never decide privilege.
//! - **Honest observation states.** A duplicate-name manifest is `ambiguous` (no digest claimed); an
//!   unobserved list is `not_observed` (an artifact state, never silence); `tools_list_complete` is
//!   never guessed `complete`.

use crate::cache::sha256_hex;
use crate::tool_decision::{classify, sanitize};
use assay_core::mcp::jcs;
use serde_json::{json, Value};

pub const SCHEMA: &str = "assay.mcp_manifest_observed.v0";
pub const CANONICALIZATION: &str = "assay.mcp_manifest_projection.v0";

/// Completeness of the observed `tools/list`. Never guessed: `complete` is only legitimate when the
/// full pagination chain (until no `nextCursor`) was observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Completeness {
    /// The full list operation was observed, including every paginated page.
    Complete,
    /// Pagination started but the chain was not completed.
    Partial,
    /// A `tools/list`-shaped response was seen but completeness cannot be proven.
    Unknown,
}

impl Completeness {
    fn as_str(self) -> &'static str {
        match self {
            Completeness::Complete => "complete",
            Completeness::Partial => "partial",
            Completeness::Unknown => "unknown",
        }
    }
}

/// Project one observed MCP tool definition into the canonical per-tool shape. Accepts the MCP wire
/// spelling (`inputSchema`/`outputSchema`) and the snake_case spelling; an absent field projects to
/// `null` (never a default), so the digest reflects exactly what was observed. The raw name and
/// metadata ride into the digest verbatim — sanitization is for *rendered* fields only, never for the
/// hashed preimage, so drift is detected faithfully.
fn tool_projection(tool: &Value) -> Value {
    let get = |k: &str| tool.get(k).cloned();
    json!({
        "name": tool.get("name").and_then(|v| v.as_str()).unwrap_or_default(),
        "description": get("description").unwrap_or(Value::Null),
        "input_schema": tool
            .get("inputSchema")
            .or_else(|| tool.get("input_schema"))
            .cloned()
            .unwrap_or(Value::Null),
        "output_schema": tool
            .get("outputSchema")
            .or_else(|| tool.get("output_schema"))
            .cloned()
            .unwrap_or(Value::Null),
        "annotations": get("annotations").unwrap_or(Value::Null),
    })
}

fn digest_of(value: &Value) -> String {
    let bytes = jcs::to_vec(value).expect("jcs canonicalization");
    format!("sha256:{}", sha256_hex(&bytes))
}

/// `tool_digest` over the canonical per-tool projection.
fn tool_digest(projection: &Value) -> String {
    digest_of(projection)
}

/// `manifest_digest` over the manifest projection, with the projection id INSIDE the hashed preimage
/// and entries sorted by `(name, tool_digest)` — order-independent, shape-pinned.
fn manifest_digest(name_digests: &[(String, String)]) -> String {
    let mut entries: Vec<(String, String)> = name_digests.to_vec();
    entries.sort();
    let tools: Vec<Value> = entries
        .into_iter()
        .map(|(name, td)| json!({ "name": name, "tool_digest": td }))
        .collect();
    digest_of(&json!({ "projection": CANONICALIZATION, "tools": tools }))
}

/// Raw tool name carries identity; duplicates make the manifest ambiguous.
fn raw_name(tool: &Value) -> String {
    tool.get("name")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

fn has_duplicate_names(tools: &[Value]) -> bool {
    let mut names: Vec<String> = tools.iter().map(raw_name).collect();
    names.sort();
    names.windows(2).any(|w| w[0] == w[1])
}

/// Per-tool entry rendered into the record: digest from the raw projection, name sanitized for
/// display, privilege classifier-derived from the tool name.
fn tool_digest_entry(tool: &Value) -> (String, String, Value) {
    let projection = tool_projection(tool);
    let digest = tool_digest(&projection);
    let raw = raw_name(tool);
    // Classifier-derived privilege: keyed on the tool name only (no call args), so the server's own
    // annotations can never decide it.
    let category = classify(&raw, &Value::Null).category;
    let privileged = category.is_some();
    let entry = json!({
        "name": sanitize(&raw),
        "tool_digest": digest.clone(),
        "privileged": privileged,
        "privilege_classification": if privileged { "classified" } else { "unclassified" },
        "action_class": category.map(Value::from).unwrap_or(Value::Null),
    });
    (raw, digest, entry)
}

fn non_claims() -> Value {
    json!([
        "does not judge whether a manifest change is malicious",
        "does not infer tools outside the observed tools/list",
        "does not detect behavior drift under identical metadata",
        "privileged is classifier-derived, not the server's own annotations"
    ])
}

/// Emit the record for an unobserved `tools/list`. An artifact state, never a missing file: a consumer
/// reads this as inconclusive, never as "no drift".
pub fn not_observed(server_id: &str) -> Value {
    json!({
        "schema": SCHEMA,
        "status": "not_observed",
        "server": { "id": sanitize(server_id) },
        "observed": {
            "manifest_digest": Value::Null,
            "canonicalization": CANONICALIZATION,
            "tool_count": 0,
            "privileged_tool_count": 0,
            "tools_list_observed": false,
            "tools_list_complete": "unknown",
            "tool_digests": []
        },
        "non_claims": non_claims()
    })
}

/// Build the observed-manifest record from observed tool definitions.
///
/// `status` is `observed` normally, `ambiguous` when the observed list has duplicate tool names (then
/// `manifest_digest` is null — an ambiguous identity is never claimed clean). `tools_list_complete` is
/// passed in by the observer and never guessed here.
pub fn build_observed(server_id: &str, tools: &[Value], completeness: Completeness) -> Value {
    let ambiguous = has_duplicate_names(tools);

    let mut name_digests: Vec<(String, String)> = Vec::with_capacity(tools.len());
    let mut entries: Vec<Value> = Vec::with_capacity(tools.len());
    let mut privileged_count = 0u64;
    for tool in tools {
        let (raw, digest, entry) = tool_digest_entry(tool);
        if entry["privileged"] == json!(true) {
            privileged_count += 1;
        }
        name_digests.push((raw, digest));
        entries.push(entry);
    }

    // A duplicate-name manifest withholds the manifest digest (ambiguous identity), but still carries
    // honest per-tool detail and counts.
    let (status, digest) = if ambiguous {
        ("ambiguous", Value::Null)
    } else {
        ("observed", Value::String(manifest_digest(&name_digests)))
    };

    json!({
        "schema": SCHEMA,
        "status": status,
        "server": { "id": sanitize(server_id) },
        "observed": {
            "manifest_digest": digest,
            "canonicalization": CANONICALIZATION,
            "tool_count": tools.len(),
            "privileged_tool_count": privileged_count,
            "tools_list_observed": true,
            "tools_list_complete": completeness.as_str(),
            "tool_digests": entries
        },
        "non_claims": non_claims()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn p60a_fixture() -> Value {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/mcp_manifest_drift/canonicalization_example.json");
        serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap()
    }

    // The two raw MCP tool definitions that, projected, equal the committed P60a canonical example.
    fn p60a_raw_tools() -> Vec<Value> {
        vec![
            json!({"name": "search", "description": "does a thing", "inputSchema": {"type": "object"}}),
            json!({
                "name": "github.add_deploy_key",
                "description": "Add a deploy key",
                "inputSchema": {"type": "object", "required": ["owner", "repo"]}
            }),
        ]
    }

    #[test]
    fn producer_reproduces_p60a_committed_digests() {
        // Cross-layer anchor: the producer's projection + digest must equal the bytes P60a committed.
        let ex = p60a_fixture();
        let deploy_proj = tool_projection(&json!({
            "name": "github.add_deploy_key",
            "description": "Add a deploy key",
            "inputSchema": {"type": "object", "required": ["owner", "repo"]}
        }));
        assert_eq!(
            tool_digest(&deploy_proj),
            ex["per_tool"]["expected_tool_digest"].as_str().unwrap(),
            "producer per-tool digest must equal the P60a committed tool_digest"
        );

        let rec = build_observed("github", &p60a_raw_tools(), Completeness::Complete);
        assert_eq!(
            rec["observed"]["manifest_digest"].as_str().unwrap(),
            ex["manifest"]["expected_manifest_digest"].as_str().unwrap(),
            "producer manifest_digest must equal the P60a committed manifest_digest"
        );
        assert_eq!(rec["status"], json!("observed"));
        assert_eq!(rec["observed"]["tools_list_complete"], json!("complete"));
    }

    #[test]
    fn description_change_changes_the_digest() {
        let base = build_observed("github", &p60a_raw_tools(), Completeness::Complete);
        let mut changed = p60a_raw_tools();
        changed[0]["description"] = json!("now cached");
        let after = build_observed("github", &changed, Completeness::Complete);
        assert_ne!(
            base["observed"]["manifest_digest"], after["observed"]["manifest_digest"],
            "a description change must move the manifest digest"
        );
    }

    #[test]
    fn manifest_digest_is_order_independent() {
        let forward = build_observed("github", &p60a_raw_tools(), Completeness::Complete);
        let mut reversed = p60a_raw_tools();
        reversed.reverse();
        let back = build_observed("github", &reversed, Completeness::Complete);
        assert_eq!(
            forward["observed"]["manifest_digest"], back["observed"]["manifest_digest"],
            "tool order must not affect the manifest digest"
        );
    }

    #[test]
    fn new_privileged_tool_increments_count_and_classifies() {
        let base = build_observed("github", &p60a_raw_tools(), Completeness::Complete);
        // p60a tools: "search" (unclassified) + "github.add_deploy_key" (privileged) -> 1 privileged.
        assert_eq!(base["observed"]["privileged_tool_count"], json!(1));

        let mut more = p60a_raw_tools();
        more.push(json!({"name": "slack.add_member", "description": "invite"}));
        let after = build_observed("github", &more, Completeness::Complete);
        assert_eq!(after["observed"]["privileged_tool_count"], json!(2));

        let added = after["observed"]["tool_digests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["name"] == json!("slack.add_member"))
            .unwrap();
        assert_eq!(added["privileged"], json!(true));
        assert_eq!(added["privilege_classification"], json!("classified"));
        assert_eq!(added["action_class"], json!("slack_add_member"));
    }

    #[test]
    fn unclassified_tool_is_not_privileged() {
        let rec = build_observed(
            "srv",
            &[json!({"name": "misc.do_thing", "description": "x"})],
            Completeness::Complete,
        );
        let e = &rec["observed"]["tool_digests"][0];
        assert_eq!(e["privileged"], json!(false));
        assert_eq!(e["privilege_classification"], json!("unclassified"));
        assert_eq!(e["action_class"], Value::Null);
        assert_eq!(rec["observed"]["privileged_tool_count"], json!(0));
    }

    #[test]
    fn duplicate_names_are_ambiguous_not_digest_clean() {
        let tools = vec![
            json!({"name": "search", "description": "a"}),
            json!({"name": "search", "description": "b"}),
        ];
        let rec = build_observed("srv", &tools, Completeness::Complete);
        assert_eq!(rec["status"], json!("ambiguous"));
        assert_eq!(rec["observed"]["manifest_digest"], Value::Null);
        // Honest detail is still carried.
        assert_eq!(rec["observed"]["tool_count"], json!(2));
        assert_eq!(rec["observed"]["tool_digests"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn not_observed_is_an_artifact_state() {
        let rec = not_observed("srv");
        assert_eq!(rec["status"], json!("not_observed"));
        assert_eq!(rec["observed"]["tools_list_observed"], json!(false));
        assert_eq!(rec["observed"]["tools_list_complete"], json!("unknown"));
        assert_eq!(rec["observed"]["manifest_digest"], Value::Null);
        assert_eq!(rec["observed"]["tool_count"], json!(0));
    }

    #[test]
    fn completeness_is_carried_not_guessed() {
        for (c, want) in [
            (Completeness::Complete, "complete"),
            (Completeness::Partial, "partial"),
            (Completeness::Unknown, "unknown"),
        ] {
            let rec = build_observed("srv", &p60a_raw_tools(), c);
            assert_eq!(rec["observed"]["tools_list_complete"], json!(want));
            // Completeness never changes the digest — only the observed metadata.
            assert!(rec["observed"]["manifest_digest"].is_string());
        }
    }

    #[test]
    fn hostile_tool_name_is_sanitized_in_display_but_hashed_raw() {
        let hostile = "tool\u{1b}[31m\u{0000}";
        let rec = build_observed(
            "srv",
            &[json!({"name": hostile, "description": "x"})],
            Completeness::Complete,
        );
        let shown = rec["observed"]["tool_digests"][0]["name"].as_str().unwrap();
        assert!(!shown.contains('\u{1b}') && !shown.contains('\u{0000}'));
        assert!(shown.contains('\u{FFFD}'));
        // The digest is over the RAW projection, so it differs from the sanitized-name projection.
        let raw_proj = tool_projection(&json!({"name": hostile, "description": "x"}));
        assert_eq!(
            rec["observed"]["tool_digests"][0]["tool_digest"]
                .as_str()
                .unwrap(),
            tool_digest(&raw_proj)
        );
    }

    #[test]
    fn schema_and_non_claims_present() {
        let rec = build_observed("srv", &p60a_raw_tools(), Completeness::Complete);
        assert_eq!(rec["schema"], json!(SCHEMA));
        assert_eq!(rec["observed"]["canonicalization"], json!(CANONICALIZATION));
        assert!(!rec["non_claims"].as_array().unwrap().is_empty());
    }
}
