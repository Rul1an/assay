//! P57b: build the observed tool-decision record (`assay.tool_decision_surface.v0`) from a proxied
//! `tools/call`. Spec: docs/reference/tool-decision-surface.md.
//!
//! Two load-bearing rules are enforced here by construction:
//!
//! - **No raw arguments or secrets ever ride in the record.** The builder never copies the call
//!   arguments into the decision; it records `arguments_redacted: true` and
//!   `secret_material_stored: false`. A credential is referenced by alias only, never by value.
//! - **Asserted is not verified.** The proxy only observes that the tool returned; it cannot prove
//!   the SaaS side effect. `side_effect_verified` is therefore always `false` here. It can only
//!   become true when separate, checked audit evidence confirms it, which the proxy never has.
//!
//! Privileged-action classification (the `github_deploy_key` / `slack_add_member` /
//! `workspace_admin` classifiers) is P57c. Until then every observed tool is `observed_unknown_tool`,
//! which is deliberately never read as clean.

use serde_json::{json, Value};

pub const SCHEMA: &str = "assay.tool_decision_surface.v0";

/// The effect the proxy decided for the call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Allow,
    Deny,
    Error,
}

impl Effect {
    fn as_str(self) -> &'static str {
        match self {
            Effect::Allow => "allow",
            Effect::Deny => "deny",
            Effect::Error => "error",
        }
    }
}

/// Inputs the proxy has at the `tool_call_done` site.
pub struct ObservedCall<'a> {
    pub server_id: &'a str,
    pub tool_name: &'a str,
    pub effect: Effect,
    /// Machine-readable status string (e.g. "success", "blocked", "error", "timeout").
    pub status: &'a str,
    /// Policy rule id if the decision named one; `None` falls back to a neutral marker.
    pub rule_id: Option<&'a str>,
}

/// Replace control characters (other than tab/newline/carriage-return) with U+FFFD, so a hostile
/// tool name cannot smuggle terminal escapes or NULs into a rendered record. Kept local to avoid a
/// new dependency; mirrors the evidence-layer terminal sanitization discipline.
pub fn sanitize(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if c.is_control() && c != '\t' && c != '\n' && c != '\r' {
                '\u{FFFD}'
            } else {
                c
            }
        })
        .collect()
}

/// Build a single observed tool-decision entry. Redaction and the asserted-vs-verified rule are
/// applied here by construction; no caller can opt out of them.
pub fn build_decision(call: &ObservedCall<'_>) -> Value {
    // A side effect is *asserted* only when the tool was allowed and returned success. It is never
    // *verified* by the proxy.
    let side_effect_asserted = matches!(call.effect, Effect::Allow) && call.status == "success";
    json!({
        "server": { "id": sanitize(call.server_id), "transport": "mcp" },
        "tool": { "name": sanitize(call.tool_name), "category": Value::Null },
        // No classifier yet (P57c): an observed-but-unclassified tool is never clean.
        "classification": "observed_unknown_tool",
        "action": { "class": "unclassified", "verb": Value::Null, "resource_type": Value::Null, "target": Value::Null },
        "decision": {
            "effect": call.effect.as_str(),
            "source": "assay-mcp-server",
            "rule_id": call.rule_id.map(sanitize).map(Value::String).unwrap_or(Value::Null),
            "enforced": true
        },
        "response": {
            "status": sanitize(call.status),
            "side_effect_asserted": side_effect_asserted,
            "side_effect_verified": false
        },
        "redaction": {
            "arguments_redacted": true,
            "credential_alias": Value::Null,
            "secret_material_stored": false
        }
    })
}

/// Wrap one or more decisions into the full `assay.tool_decision_surface.v0` surface.
pub fn surface(decisions: Vec<Value>) -> Value {
    json!({
        "schema": SCHEMA,
        "observed_tool_decisions": decisions,
        "non_claims": [
            "does not prove SaaS-side persistence without external audit evidence",
            "does not infer tool actions outside observed MCP proxy traffic",
            "does not expose raw secrets or tokens"
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call<'a>(tool: &'a str, effect: Effect, status: &'a str) -> ObservedCall<'a> {
        ObservedCall {
            server_id: "github",
            tool_name: tool,
            effect,
            status,
            rule_id: Some("r1"),
        }
    }

    #[test]
    fn allowed_success_asserts_but_never_verifies_the_side_effect() {
        let d = build_decision(&call("github.add_deploy_key", Effect::Allow, "success"));
        assert_eq!(d["response"]["side_effect_asserted"], json!(true));
        assert_eq!(d["response"]["side_effect_verified"], json!(false));
    }

    #[test]
    fn denied_call_asserts_no_side_effect() {
        let d = build_decision(&call("github.add_deploy_key", Effect::Deny, "blocked"));
        assert_eq!(d["response"]["side_effect_asserted"], json!(false));
        assert_eq!(d["response"]["side_effect_verified"], json!(false));
    }

    #[test]
    fn arguments_and_secrets_are_never_in_the_record() {
        // build_decision takes no arguments at all, so a secret in the call cannot leak; the record
        // states it explicitly.
        let d = build_decision(&call("misc.do_thing", Effect::Allow, "success"));
        let text = serde_json::to_string(&d).unwrap();
        assert!(
            !text.contains("ghp_"),
            "no token-shaped material in the record"
        );
        assert_eq!(d["redaction"]["arguments_redacted"], json!(true));
        assert_eq!(d["redaction"]["secret_material_stored"], json!(false));
        assert_eq!(d["redaction"]["credential_alias"], json!(null));
    }

    #[test]
    fn unclassified_tool_is_observed_unknown_never_clean() {
        let d = build_decision(&call("misc.do_thing", Effect::Allow, "success"));
        assert_eq!(d["classification"], json!("observed_unknown_tool"));
        assert_eq!(d["action"]["class"], json!("unclassified"));
    }

    #[test]
    fn hostile_strings_are_sanitized() {
        let hostile = "tool\u{1b}[31m\u{0000}";
        let d = build_decision(&call(hostile, Effect::Allow, "success"));
        let name = d["tool"]["name"].as_str().unwrap();
        assert!(
            !name.contains('\u{1b}') && !name.contains('\u{0000}'),
            "control chars sanitized"
        );
        assert!(name.contains('\u{FFFD}'));
    }

    #[test]
    fn surface_carries_schema_and_non_claims() {
        let s = surface(vec![build_decision(&call("t", Effect::Allow, "success"))]);
        assert_eq!(s["schema"], json!(SCHEMA));
        assert!(s["non_claims"]
            .as_array()
            .map(|a| !a.is_empty())
            .unwrap_or(false));
        assert_eq!(s["observed_tool_decisions"].as_array().unwrap().len(), 1);
    }
}
