use crate::fingerprint::sha256_hex;
use crate::mcp::jcs;
use serde_json::{json, Value};

use super::verdict::verdict_rank;
use super::{is_sha256_digest, SCHEMA};

/// Recipe id for the pack row (experimental).
pub const RECIPE: &str = "tool_decision_truth.v0";

/// Whether `s` is one of the four lattice verdicts.
fn is_verdict(s: &str) -> bool {
    matches!(s, "match" | "incomplete" | "mismatch" | "invalid")
}

/// A run verdict must be at least as severe as the carrier's own decision verdict.
fn run_verdict_covers_carrier(carrier: &Value, run_verdict: &str) -> bool {
    match carrier.get("decision_verdict") {
        None | Some(Value::Null) => true,
        Some(Value::String(cv)) if is_verdict(cv) => verdict_rank(run_verdict) >= verdict_rank(cv),
        _ => false,
    }
}

/// Digest over the decision identity (the two pinned digests). This is the stable logical handle that
/// JOINS a pack row to a carrier; it is NOT the carrier content digest.
pub fn decision_identity_digest(
    observed_input_digest: &str,
    declared_policy_digest: &str,
) -> Option<String> {
    let identity = json!({
        "observed_input_digest": observed_input_digest,
        "declared_policy_digest": declared_policy_digest,
    });
    Some(format!(
        "sha256:{}",
        sha256_hex(&jcs::to_string(&identity).ok()?)
    ))
}

/// Digest over the FULL canonical carrier record bytes.
pub fn carrier_content_digest(carrier: &Value) -> Option<String> {
    Some(format!(
        "sha256:{}",
        sha256_hex(&jcs::to_string(carrier).ok()?)
    ))
}

/// The cross-ecosystem envelope a pack row cites the carrier by.
pub fn evidence_ref(carrier_content_digest: &str, reference: &str) -> Value {
    json!({
        "type": "tool_decision_truth",
        "digest": carrier_content_digest,
        "digest_subject": "carrier_content",
        "canonicalization": "jcs-json-v1",
        "schema": SCHEMA,
        "ref": reference,
    })
}

/// Build a proven recipe row binding a real carrier into the existing Evidence Pack (no pack v2).
pub fn pack_recipe_row(carrier: &Value, run_verdict: &str, reference: &str) -> Option<Value> {
    if !is_verdict(run_verdict) || !run_verdict_covers_carrier(carrier, run_verdict) {
        return None;
    }
    let oid = carrier
        .get("observed_input_digest")
        .and_then(|v| v.as_str())?;
    let dpd = carrier
        .get("declared_policy_digest")
        .and_then(|v| v.as_str())?;
    if !is_sha256_digest(oid) || !is_sha256_digest(dpd) {
        return None;
    }
    let content = carrier_content_digest(carrier)?;
    let identity = decision_identity_digest(oid, dpd)?;
    let er = evidence_ref(&content, reference);
    let binding_input = json!({
        "recipe": RECIPE,
        "evidence_ref": er,
        "decision_identity_digest": identity,
        "run_verdict": run_verdict,
    });
    let coherence_binding = format!(
        "sha256:{}",
        sha256_hex(&jcs::to_string(&binding_input).ok()?)
    );
    Some(json!({
        "recipe": RECIPE,
        "evidence_ref": er,
        "decision_identity_digest": identity,
        "run_verdict": run_verdict,
        "coherence_binding": coherence_binding,
    }))
}

/// Verify a recipe row coheres with the carrier it cites.
pub fn verify_recipe_row(row: &Value, carrier: &Value, run_verdict: &str) -> bool {
    if row.get("recipe").and_then(|r| r.as_str()) != Some(RECIPE) {
        return false;
    }
    let Some(er) = row.get("evidence_ref").cloned() else {
        return false;
    };
    if er.get("type").and_then(|x| x.as_str()) != Some("tool_decision_truth")
        || er.get("schema").and_then(|x| x.as_str()) != Some(SCHEMA)
        || er.get("canonicalization").and_then(|x| x.as_str()) != Some("jcs-json-v1")
        || er.get("digest_subject").and_then(|x| x.as_str()) != Some("carrier_content")
    {
        return false;
    }
    if row.get("run_verdict").and_then(|v| v.as_str()) != Some(run_verdict) {
        return false;
    }
    if !is_verdict(run_verdict) || !run_verdict_covers_carrier(carrier, run_verdict) {
        return false;
    }
    let Some(er_digest) = er.get("digest").and_then(|d| d.as_str()) else {
        return false;
    };
    let Some(row_identity) = row.get("decision_identity_digest").and_then(|d| d.as_str()) else {
        return false;
    };
    if !is_sha256_digest(er_digest) || !is_sha256_digest(row_identity) {
        return false;
    }
    match carrier_content_digest(carrier) {
        Some(c) if c == er_digest => {}
        _ => return false,
    }
    let (Some(oid), Some(dpd)) = (
        carrier
            .get("observed_input_digest")
            .and_then(|v| v.as_str()),
        carrier
            .get("declared_policy_digest")
            .and_then(|v| v.as_str()),
    ) else {
        return false;
    };
    if !is_sha256_digest(oid) || !is_sha256_digest(dpd) {
        return false;
    }
    match decision_identity_digest(oid, dpd) {
        Some(i) if i == row_identity => {}
        _ => return false,
    }
    let binding_input = json!({
        "recipe": RECIPE,
        "evidence_ref": er,
        "decision_identity_digest": row_identity,
        "run_verdict": run_verdict,
    });
    let expected = match jcs::to_string(&binding_input) {
        Ok(s) => format!("sha256:{}", sha256_hex(&s)),
        Err(_) => return false,
    };
    row.get("coherence_binding").and_then(|c| c.as_str()) == Some(expected.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::policy::McpPolicy;
    use crate::mcp::tool_decision_truth::verdict::{build_classified_record, DecisionEvidence};

    const REF: &str = "audit://decision/c1";

    fn pack_policy(allow: &str, deny: &str) -> McpPolicy {
        serde_json::from_value(json!({
            "version": "1",
            "tools": {"allow": [allow], "deny": [deny]},
            "schemas": {"deploy": {"type": "object", "required": ["env"],
                "properties": {"env": {"enum": ["staging", "prod"]}}}},
            "enforcement": {"unconstrained_tools": "warn"}
        }))
        .unwrap()
    }

    /// A real, fully-classified carrier (real declared digest + decision_verdict): a row cites THIS.
    fn carrier() -> Value {
        build_classified_record(
            &pack_policy("deploy", "delete_all"),
            "deploy",
            &json!({"env": "prod"}),
            0,
            b"reference-test-key-v0",
            "test-key-v0",
            "authoritative_boundary",
            "c1",
            "ok",
            "present",
            &DecisionEvidence::default(),
        )
        .unwrap()
    }

    #[test]
    fn evidence_ref_has_the_envelope_fields() {
        let cd = carrier_content_digest(&carrier()).unwrap();
        let er = evidence_ref(&cd, REF);
        let mut keys: Vec<&String> = er.as_object().unwrap().keys().collect();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "canonicalization",
                "digest",
                "digest_subject",
                "ref",
                "schema",
                "type"
            ]
        );
        assert_eq!(er["schema"], json!(SCHEMA));
        assert_eq!(er["digest"], json!(cd));
        assert_eq!(er["digest_subject"], json!("carrier_content"));
    }

    #[test]
    fn content_and_identity_digests_are_distinct_and_well_formed() {
        let c = carrier();
        let content = carrier_content_digest(&c).unwrap();
        let identity = decision_identity_digest(
            c["observed_input_digest"].as_str().unwrap(),
            c["declared_policy_digest"].as_str().unwrap(),
        )
        .unwrap();
        assert_ne!(content, identity);
        assert!(super::super::is_sha256_digest(&content));
        assert!(super::super::is_sha256_digest(&identity));
    }

    #[test]
    fn recipe_row_coheres_with_its_real_carrier() {
        let c = carrier();
        let row = pack_recipe_row(&c, "match", REF).unwrap();
        assert!(verify_recipe_row(&row, &c, "match"));
        assert_eq!(
            row["evidence_ref"]["digest"],
            json!(carrier_content_digest(&c).unwrap())
        );
        assert!(row.get("decision_identity_digest").is_some());
    }

    #[test]
    fn tampering_any_carrier_field_or_the_verdict_fails_closed() {
        let c = carrier();
        let row = pack_recipe_row(&c, "match", REF).unwrap();
        let mut tampered = c.clone();
        tampered["result_status"] = json!("error");
        assert!(!verify_recipe_row(&row, &tampered, "match"));
        let mut tampered_id = c.clone();
        tampered_id["observed_input_digest"] = json!("sha256:deadbeef");
        assert!(!verify_recipe_row(&row, &tampered_id, "match"));
        assert!(!verify_recipe_row(&row, &c, "mismatch"));
    }

    /// A row whose coherence_binding is self-consistent over the given (possibly foreign) recipe/envelope.
    fn coherent_row(recipe: &str, er: Value, identity: &str, run_verdict: &str) -> Value {
        let binding_input = json!({
            "recipe": recipe,
            "evidence_ref": er,
            "decision_identity_digest": identity,
            "run_verdict": run_verdict,
        });
        let cb = format!(
            "sha256:{}",
            crate::fingerprint::sha256_hex(&crate::mcp::jcs::to_string(&binding_input).unwrap())
        );
        json!({
            "recipe": recipe,
            "evidence_ref": er,
            "decision_identity_digest": identity,
            "run_verdict": run_verdict,
            "coherence_binding": cb,
        })
    }

    #[test]
    fn verify_rejects_foreign_recipe_or_envelope_even_when_self_coherent() {
        let c = carrier();
        let content = carrier_content_digest(&c).unwrap();
        let identity = decision_identity_digest(
            c["observed_input_digest"].as_str().unwrap(),
            c["declared_policy_digest"].as_str().unwrap(),
        )
        .unwrap();
        let good_er = evidence_ref(&content, REF);
        assert!(verify_recipe_row(
            &coherent_row(RECIPE, good_er.clone(), &identity, "match"),
            &c,
            "match"
        ));
        assert!(!verify_recipe_row(
            &coherent_row("other.recipe.v0", good_er.clone(), &identity, "match"),
            &c,
            "match"
        ));
        for (field, bad) in [
            ("type", json!("other")),
            ("schema", json!("other.schema.v0")),
            ("canonicalization", json!("cbor-deterministic-v1")),
            ("digest_subject", json!("decision_identity")),
        ] {
            let mut er = good_er.as_object().unwrap().clone();
            er.insert(field.to_string(), bad);
            let row = coherent_row(RECIPE, Value::Object(er), &identity, "match");
            assert!(
                !verify_recipe_row(&row, &c, "match"),
                "envelope field {field} must be rejected"
            );
        }
    }

    #[test]
    fn verify_rejects_malformed_citation_digests() {
        let c = carrier();
        let identity = decision_identity_digest(
            c["observed_input_digest"].as_str().unwrap(),
            c["declared_policy_digest"].as_str().unwrap(),
        )
        .unwrap();
        let bad_er = evidence_ref("sha256:short", REF);
        assert!(!verify_recipe_row(
            &coherent_row(RECIPE, bad_er, &identity, "match"),
            &c,
            "match"
        ));
        let good_er = evidence_ref(&carrier_content_digest(&c).unwrap(), REF);
        assert!(!verify_recipe_row(
            &coherent_row(RECIPE, good_er, "not-a-digest", "match"),
            &c,
            "match"
        ));
    }

    #[test]
    fn verify_rejects_carrier_with_malformed_embedded_digest() {
        let mut bad = carrier();
        bad["observed_input_digest"] = json!("sha256:bad");
        let content = carrier_content_digest(&bad).unwrap();
        let identity = decision_identity_digest(
            "sha256:bad",
            bad["declared_policy_digest"].as_str().unwrap(),
        )
        .unwrap();
        let row = coherent_row(RECIPE, evidence_ref(&content, REF), &identity, "match");
        assert!(!verify_recipe_row(&row, &bad, "match"));
    }

    #[test]
    fn pack_row_rejects_bogus_or_understated_verdict() {
        let c = carrier();
        assert_eq!(c["decision_verdict"], json!("match"));
        assert!(pack_recipe_row(&c, "approved", REF).is_none());

        let mismatch_carrier = build_classified_record(
            &pack_policy("deploy", "delete_all"),
            "delete_all",
            &json!({}),
            0,
            b"reference-test-key-v0",
            "test-key-v0",
            "authoritative_boundary",
            "c1",
            "ok",
            "present",
            &DecisionEvidence::default(),
        )
        .unwrap();
        assert_eq!(mismatch_carrier["decision_verdict"], json!("mismatch"));
        assert!(pack_recipe_row(&mismatch_carrier, "match", REF).is_none());
        let row = pack_recipe_row(&mismatch_carrier, "mismatch", REF).unwrap();
        assert!(verify_recipe_row(&row, &mismatch_carrier, "mismatch"));

        let content = carrier_content_digest(&mismatch_carrier).unwrap();
        let identity = decision_identity_digest(
            mismatch_carrier["observed_input_digest"].as_str().unwrap(),
            mismatch_carrier["declared_policy_digest"].as_str().unwrap(),
        )
        .unwrap();
        let understating = coherent_row(RECIPE, evidence_ref(&content, REF), &identity, "match");
        assert!(!verify_recipe_row(
            &understating,
            &mismatch_carrier,
            "match"
        ));

        let mut malformed = carrier();
        malformed["decision_verdict"] = json!("approved");
        assert!(pack_recipe_row(&malformed, "match", REF).is_none());
        assert!(pack_recipe_row(&malformed, "invalid", REF).is_none());
    }
}
