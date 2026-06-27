use crate::mcp::policy::{ArgsCheck, McpPolicy, UnconstrainedMode};
use serde_json::{json, Value};

use super::digest::build_record;

/// Lattice rank: `invalid > mismatch > incomplete > match`. Unknown strings rank as `invalid`.
pub(super) fn verdict_rank(v: &str) -> u8 {
    match v {
        "match" => 0,
        "incomplete" => 1,
        "mismatch" => 2,
        _ => 3,
    }
}

fn to_static_verdict(v: &str) -> &'static str {
    match v {
        "match" => "match",
        "incomplete" => "incomplete",
        "mismatch" => "mismatch",
        _ => "invalid",
    }
}

fn enforcement_axis(mode: &UnconstrainedMode) -> &'static str {
    match mode {
        UnconstrainedMode::Deny => "mismatch",
        UnconstrainedMode::Allow => "match",
        UnconstrainedMode::Warn => "incomplete",
    }
}

fn nonempty(list: &Option<Vec<String>>) -> &[String] {
    list.as_deref().unwrap_or(&[])
}

fn tool_axis(policy: &McpPolicy, tool_name: &str) -> &'static str {
    let matches = |p: &String| McpPolicy::tool_name_matches_experimental(tool_name, p);
    if nonempty(&policy.tools.deny).iter().any(matches) {
        return "mismatch";
    }
    let allow = nonempty(&policy.tools.allow);
    if !allow.is_empty() {
        return if allow.iter().any(matches) {
            "match"
        } else {
            "mismatch"
        };
    }
    enforcement_axis(&policy.enforcement.unconstrained_tools)
}

fn args_axis(policy: &McpPolicy, tool_name: &str, args: Option<&Value>) -> &'static str {
    let Some(args) = args else {
        return "incomplete";
    };
    match policy.check_tool_args_experimental(tool_name, args) {
        ArgsCheck::NoSchema => enforcement_axis(&policy.enforcement.unconstrained_tools),
        ArgsCheck::Valid => "match",
        ArgsCheck::Invalid => "mismatch",
        ArgsCheck::Malformed => "invalid",
    }
}

fn identity_axis(identity_state: &str) -> &'static str {
    match identity_state {
        "present" | "absent" => "match",
        "required_missing" => "incomplete",
        _ => "invalid",
    }
}

/// Class allow/deny axis. Not declared -> not applicable (`match`). Declared but no tool-class evidence
/// -> `incomplete`. Otherwise denied/missing classes become `mismatch`.
fn class_axis(policy: &McpPolicy, tool_classes: Option<&[String]>) -> &'static str {
    let deny_c = nonempty(&policy.tools.deny_classes);
    let allow_c = nonempty(&policy.tools.allow_classes);
    if deny_c.is_empty() && allow_c.is_empty() {
        return "match";
    }
    let Some(tc) = tool_classes else {
        return "incomplete";
    };
    if !deny_c.is_empty() && tc.iter().any(|c| deny_c.contains(c)) {
        return "mismatch";
    }
    if !allow_c.is_empty() && !tc.iter().any(|c| allow_c.contains(c)) {
        return "mismatch";
    }
    "match"
}

enum Applicability {
    Applicable,
    NotApplicable,
    Undeterminable,
}

fn applicability(
    names: &[String],
    classes: &[String],
    tool_name: &str,
    tool_classes: Option<&[String]>,
) -> Applicability {
    if names
        .iter()
        .any(|p| McpPolicy::tool_name_matches_experimental(tool_name, p))
    {
        return Applicability::Applicable;
    }
    if classes.is_empty() {
        return Applicability::NotApplicable;
    }
    match tool_classes {
        None => Applicability::Undeterminable,
        Some(tc) if tc.iter().any(|c| classes.contains(c)) => Applicability::Applicable,
        Some(_) => Applicability::NotApplicable,
    }
}

fn obligation_axis(applic: Applicability, satisfied: Option<bool>) -> &'static str {
    match applic {
        Applicability::NotApplicable => "match",
        Applicability::Undeterminable => "incomplete",
        Applicability::Applicable => match satisfied {
            None => "incomplete",
            Some(true) => "match",
            Some(false) => "mismatch",
        },
    }
}

/// EXPERIMENTAL: runtime evidence the verdict gate needs to decide declared constraints beyond the tool
/// name, args schema, and identity.
#[derive(Debug, Clone, Default)]
pub struct DecisionEvidence {
    pub tool_classes: Option<Vec<String>>,
    pub approval_obtained: Option<bool>,
    pub scope_satisfied: Option<bool>,
    pub redaction_applied: Option<bool>,
}

/// Per-decision verdict over the declared policy: the lattice-max of every axis the declared digest binds
/// (tool name, args schema, identity, classes, approval, scope, redaction).
pub fn decision_verdict(
    policy: &McpPolicy,
    tool_name: &str,
    args: Option<&Value>,
    identity_state: &str,
    evidence: &DecisionEvidence,
) -> &'static str {
    let p = policy.normalized_declared_view_experimental();
    let tc = evidence.tool_classes.as_deref();
    let approval = applicability(
        nonempty(&p.tools.approval_required),
        nonempty(&p.tools.approval_required_classes),
        tool_name,
        tc,
    );
    let scope = applicability(
        nonempty(&p.tools.restrict_scope),
        nonempty(&p.tools.restrict_scope_classes),
        tool_name,
        tc,
    );
    let redaction = applicability(
        nonempty(&p.tools.redact_args),
        nonempty(&p.tools.redact_args_classes),
        tool_name,
        tc,
    );
    [
        tool_axis(&p, tool_name),
        args_axis(&p, tool_name, args),
        identity_axis(identity_state),
        class_axis(&p, tc),
        obligation_axis(approval, evidence.approval_obtained),
        obligation_axis(scope, evidence.scope_satisfied),
        obligation_axis(redaction, evidence.redaction_applied),
    ]
    .into_iter()
    .max_by_key(|&v| verdict_rank(v))
    .unwrap_or("match")
}

/// Run-level verdict: the lattice-max over per-decision verdicts, plus order integrity.
pub fn run_verdict(decision_verdicts: &[&str], orders: &[i64]) -> &'static str {
    if decision_verdicts.len() != orders.len() {
        return "invalid";
    }
    let mut seen = std::collections::HashSet::new();
    for o in orders {
        if !seen.insert(*o) {
            return "invalid";
        }
    }
    let mut worst: &'static str = "match";
    for v in decision_verdicts {
        let s = to_static_verdict(v);
        if verdict_rank(s) > verdict_rank(worst) {
            worst = s;
        }
    }
    worst
}

/// Build a fully-classified carrier record and compute the [`decision_verdict`] with supplied evidence.
#[allow(clippy::too_many_arguments)]
pub fn build_classified_record(
    policy: &McpPolicy,
    tool_name: &str,
    args: &Value,
    order: i64,
    key: &[u8],
    key_id: &str,
    source_class: &str,
    call_id: &str,
    result_status: &str,
    identity_state: &str,
    evidence: &DecisionEvidence,
) -> Option<Value> {
    let declared = policy.declared_constraint_digest_experimental()?;
    let mut record = build_record(
        tool_name,
        args,
        order,
        &declared,
        key,
        key_id,
        source_class,
        call_id,
        result_status,
        identity_state,
    )?;
    record["decision_verdict"] = json!(decision_verdict(
        policy,
        tool_name,
        Some(args),
        identity_state,
        evidence
    ));
    Some(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8] = b"k";
    const KID: &str = "kid";

    fn policy() -> McpPolicy {
        serde_json::from_value(json!({
            "version": "1",
            "tools": {"allow": ["read_file", "deploy"], "deny": ["delete_all"]},
            "schemas": {"deploy": {"type": "object", "required": ["env"],
                "properties": {"env": {"enum": ["staging", "prod"]}}}},
            "enforcement": {"unconstrained_tools": "warn"}
        }))
        .unwrap()
    }

    fn v(tool: &str, args: Option<Value>, id: &str) -> &'static str {
        decision_verdict(
            &policy(),
            tool,
            args.as_ref(),
            id,
            &DecisionEvidence::default(),
        )
    }

    fn p_from(v: Value) -> McpPolicy {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn per_decision_verdict_matrix() {
        assert_eq!(
            v("deploy", Some(json!({"env": "staging"})), "present"),
            "match"
        );
        assert_eq!(v("delete_all", Some(json!({})), "present"), "mismatch");
        assert_eq!(v("exfiltrate", Some(json!({})), "present"), "mismatch");
        assert_eq!(
            v("deploy", Some(json!({"env": "dev"})), "present"),
            "mismatch"
        );
        assert_eq!(v("deploy", None, "present"), "incomplete");
        assert_eq!(
            v("read_file", Some(json!({"path": "/x"})), "present"),
            "incomplete"
        );
        assert_eq!(
            v(
                "deploy",
                Some(json!({"env": "staging"})),
                "required_missing"
            ),
            "incomplete"
        );
        assert_eq!(
            v("deploy", Some(json!({"env": "staging"})), "invalid"),
            "invalid"
        );
    }

    #[test]
    fn absent_identity_does_not_block_match() {
        assert_eq!(v("deploy", Some(json!({"env": "prod"})), "absent"), "match");
    }

    #[test]
    fn tool_axis_uses_engine_pattern_semantics() {
        let p = p_from(json!({
            "version": "1",
            "tools": {"deny": ["delete_*"]},
            "enforcement": {"unconstrained_tools": "allow"}
        }));
        let e = DecisionEvidence::default();
        assert_eq!(
            decision_verdict(&p, "delete_all", Some(&json!({})), "present", &e),
            "mismatch"
        );
        assert_eq!(
            decision_verdict(&p, "read_file", Some(&json!({})), "present", &e),
            "match"
        );
    }

    #[test]
    fn declared_constraint_without_evidence_is_incomplete_not_match() {
        let p = p_from(json!({
            "version": "1",
            "tools": {"allow": ["pay"], "approval_required": ["pay"]},
            "enforcement": {"unconstrained_tools": "allow"}
        }));
        assert_eq!(
            decision_verdict(
                &p,
                "pay",
                Some(&json!({})),
                "present",
                &DecisionEvidence::default()
            ),
            "incomplete"
        );
        let approved = DecisionEvidence {
            approval_obtained: Some(true),
            ..Default::default()
        };
        assert_eq!(
            decision_verdict(&p, "pay", Some(&json!({})), "present", &approved),
            "match"
        );
        let denied = DecisionEvidence {
            approval_obtained: Some(false),
            ..Default::default()
        };
        assert_eq!(
            decision_verdict(&p, "pay", Some(&json!({})), "present", &denied),
            "mismatch"
        );
    }

    #[test]
    fn class_axis_needs_class_evidence() {
        let p = p_from(json!({
            "version": "1",
            "tools": {"allow": ["x"], "deny_classes": ["network"]},
            "enforcement": {"unconstrained_tools": "allow"}
        }));
        assert_eq!(
            decision_verdict(
                &p,
                "x",
                Some(&json!({})),
                "present",
                &DecisionEvidence::default()
            ),
            "incomplete"
        );
        let net = DecisionEvidence {
            tool_classes: Some(vec!["network".into()]),
            ..Default::default()
        };
        assert_eq!(
            decision_verdict(&p, "x", Some(&json!({})), "present", &net),
            "mismatch"
        );
        let fs = DecisionEvidence {
            tool_classes: Some(vec!["fs".into()]),
            ..Default::default()
        };
        assert_eq!(
            decision_verdict(&p, "x", Some(&json!({})), "present", &fs),
            "match"
        );
    }

    #[test]
    fn malformed_declared_schema_is_invalid_not_panic() {
        let p = p_from(json!({
            "version": "1",
            "tools": {"allow": ["t"]},
            "schemas": {"t": {"$ref": "#/$defs/missing"}},
            "enforcement": {"unconstrained_tools": "allow"}
        }));
        assert_eq!(
            p.check_tool_args_experimental("t", &json!({})),
            ArgsCheck::Malformed
        );
        assert_eq!(
            decision_verdict(
                &p,
                "t",
                Some(&json!({})),
                "present",
                &DecisionEvidence::default()
            ),
            "invalid"
        );
    }

    #[test]
    fn legacy_constraints_are_evaluated_by_the_verdict() {
        let p = p_from(json!({
            "version": "1",
            "tools": {"allow": ["deploy"]},
            "constraints": [{"tool": "deploy", "params": {"env": {"matches": "^prod$"}}}],
            "enforcement": {"unconstrained_tools": "allow"}
        }));
        let e = DecisionEvidence::default();
        assert_eq!(
            decision_verdict(&p, "deploy", Some(&json!({"env": "prod"})), "present", &e),
            "match"
        );
        assert_eq!(
            decision_verdict(
                &p,
                "deploy",
                Some(&json!({"env": "staging"})),
                "present",
                &e
            ),
            "mismatch"
        );
    }

    #[test]
    fn run_lattice_and_order_integrity() {
        assert_eq!(run_verdict(&["match", "incomplete"], &[0, 1]), "incomplete");
        assert_eq!(run_verdict(&["match", "mismatch"], &[0, 1]), "mismatch");
        assert_eq!(
            run_verdict(&["match", "mismatch", "invalid"], &[0, 1, 2]),
            "invalid"
        );
        assert_eq!(run_verdict(&["match", "match"], &[0, 0]), "invalid");
        assert_eq!(run_verdict(&["match"], &[0]), "match");
        assert_eq!(run_verdict(&["match", "match"], &[0]), "invalid");
        assert_eq!(run_verdict(&["match"], &[0, 1]), "invalid");
    }

    #[test]
    fn build_classified_record_carries_the_verdict() {
        let e = DecisionEvidence::default();
        let m = build_classified_record(
            &policy(),
            "deploy",
            &json!({"env": "staging"}),
            0,
            KEY,
            KID,
            "authoritative_boundary",
            "c1",
            "ok",
            "present",
            &e,
        )
        .unwrap();
        assert_eq!(m["decision_verdict"], json!("match"));
        let mm = build_classified_record(
            &policy(),
            "delete_all",
            &json!({}),
            0,
            KEY,
            KID,
            "authoritative_boundary",
            "c1",
            "ok",
            "present",
            &e,
        )
        .unwrap();
        assert_eq!(mm["decision_verdict"], json!("mismatch"));
    }
}
