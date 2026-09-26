use super::fixtures::*;
use super::*;
use crate::claims_backstop::assert_no_unearned_status;
use assay_mcp_server::tool_decision::{classify, sanitize};
use std::collections::BTreeSet;

// ---- ADR-043 §2 wire status claims on the POLICY DECISION CONTRACT (#2232) ------------------
//
// `decision_record` is Assay's own words reaching evidence and consumers: the `reason`,
// `decision`, `drift_state`, `action_class`/`verb`/`resource_type` leaves and the static
// `non_claims`. The generated record therefore gets the same closed-set backstop as the
// handshake, through the single implementation in `crate::claims_backstop` — the same file
// compiled into the library's test build — one list, one meaning.
//
// Value domain: exactly those Assay-authored leaves. The reflected operator/caller content —
// the caller id, the tool name, the credential alias, and the projected call target — is the
// caller's data, not Assay's assertion (the same reason the `tool_identity` guard pins
// reflection instead of scanning words, #3082, and the SARIF guard excises tool names,
// #3199), so it is excised per field before the scan and pinned byte-exact below instead.

/// The Assay-authored leaves of a generated decision record: the reflected operator/caller
/// fields are blanked whole (each field IS the reflection), never document-wide, so an
/// identical word in any Assay-authored leaf still trips the backstop.
fn assay_authored_leaves(record: &Value) -> Value {
    let mut scrubbed = record.clone();
    scrubbed["caller"]["id"] = Value::String(String::new());
    scrubbed["tool"]["name"] = Value::String(String::new());
    scrubbed["credential_alias"] = Value::String(String::new());
    scrubbed["action"]["target"] = Value::String(String::new());
    scrubbed
}

/// The excised fields must still reflect their inputs byte-exactly: an Assay-added suffix is
/// detected by equality, not by a word list. The target must carry the classifier projection
/// unchanged (stated as consistency between the two emission points, not as hash correctness).
fn assert_reflection_byte_exact(
    case: &str,
    policy: &EnforcePolicy,
    tool: &str,
    args: &Value,
    record: &Value,
) {
    assert_eq!(
        record["tool"]["name"],
        sanitize(tool),
        "{case}: tool name must be reflected byte-exactly"
    );
    assert_eq!(
        record["caller"]["id"],
        sanitize(&policy.caller.id),
        "{case}: caller id must be reflected byte-exactly"
    );
    let expected_alias = policy
        .upstream_credential
        .as_ref()
        .map(|c| sanitize(&c.alias));
    assert_eq!(
        record["credential_alias"],
        json!(expected_alias),
        "{case}: credential alias must be reflected byte-exactly (or null)"
    );
    assert_eq!(
        record["action"]["target"],
        classify(tool, args).target,
        "{case}: target must carry the classifier projection unchanged"
    );
}

/// Every GENERATED decision record, one input per decision/reason branch, iterated from
/// `golden_corpus` — the same table the golden contract fixture regenerates from, so a new
/// reason joins this scan from the moment its corpus row is added.
#[test]
fn decision_records_assert_no_unearned_status() {
    let mut reasons = BTreeSet::new();
    for c in golden_corpus() {
        let d = decide(&c.policy, &c.baseline, &c.observed, c.tool, &c.args);
        assert_eq!(
            d.reason, c.reason,
            "{}: corpus row must really exercise its branch",
            c.name
        );
        let rec = decision_record(&c.policy, &d, c.tool, &c.args);
        assert_reflection_byte_exact(c.name, &c.policy, c.tool, &c.args, &rec);
        assert_no_unearned_status(
            &format!("decision record {}", c.name),
            &assay_authored_leaves(&rec),
        );
        reasons.insert(d.reason);
    }
    // Branch inventory: every outcome `decide` can emit. The corpus holds twelve rows; the two
    // allowance scenarios share `no_declared_allowance` and the two incomplete-observation
    // scenarios share `manifest_current_observation_incomplete`, hence ten distinct records.
    // A new branch must name itself here to join the scan.
    let expected: BTreeSet<&str> = [
        "unclassified_tool_call",
        "classification_incomplete",
        "no_declared_allowance",
        "credential_scope_unknown",
        "credential_scope_insufficient",
        "manifest_baseline_missing",
        "manifest_current_observation_incomplete",
        "manifest_observation_ambiguous",
        "manifest_drifted_since_approval",
        "allow",
    ]
    .into_iter()
    .collect();
    assert_eq!(
        reasons, expected,
        "a new decision branch must join the scan explicitly"
    );
}

/// Status-like words in reflected content are upstream/operator data, not an Assay claim. The
/// record must reflect them verbatim, and the guard must still pass on that document —
/// otherwise it has become content censorship rather than a check on what Assay asserts.
#[test]
fn decision_record_reflects_status_like_caller_content_unchanged() {
    let policy = policy_from(VALID).unwrap();
    // Unclassified tool: the status-like name is reflected, never classified.
    let tool = "certified_partner_export";
    let d = decide_match(&policy, tool, &json!({}));
    assert_eq!(d.reason, "unclassified_tool_call");
    let rec = decision_record(&policy, &d, tool, &json!({}));
    assert_reflection_byte_exact("status-like tool name", &policy, tool, &json!({}), &rec);
    assert_no_unearned_status(
        "decision record with status-like tool name",
        &assay_authored_leaves(&rec),
    );
    // Classified call whose caller-supplied owner carries a status-like word: the allowance
    // gate denies, and the projected target still reflects the supplied owner byte-exactly.
    let args = json!({"owner": "approved-acme", "repo": "prod-app"});
    let d = decide_match(&policy, TOOL, &args);
    assert_eq!(d.reason, "no_declared_allowance");
    let rec = decision_record(&policy, &d, TOOL, &args);
    assert_eq!(rec["action"]["target"]["owner"], "approved-acme");
    assert_reflection_byte_exact("status-like target owner", &policy, TOOL, &args, &rec);
    assert_no_unearned_status(
        "decision record with status-like target owner",
        &assay_authored_leaves(&rec),
    );
}

/// Control for the excision above: the same status-like record scanned WITHOUT excising the
/// reflection must fire — otherwise the excision would be dead code and the acceptance case
/// would prove nothing.
#[test]
#[should_panic(expected = "asserts `certified`")]
fn decision_record_guard_fires_on_unexcised_reflection() {
    let policy = policy_from(VALID).unwrap();
    let tool = "certified_partner_export";
    let d = decide_match(&policy, tool, &json!({}));
    let rec = decision_record(&policy, &d, tool, &json!({}));
    assert_no_unearned_status("control", &rec);
}
