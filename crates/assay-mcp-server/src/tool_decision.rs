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

use crate::cache::sha256_hex;
use serde_json::{json, Value};

pub const SCHEMA: &str = "assay.tool_decision_surface.v0";

/// Domain-separated, normalized pseudonym for a sensitive identifier. The value is trimmed and
/// lowercased, then hashed under a per-field domain (`assay.tool_target.v0:<domain>:<value>`) so a
/// hash from one field can never collide with another, and the raw value is never stored.
///
/// This is pseudonymization, not anonymization: equal inputs yield equal hashes. The only claim is
/// that the raw argument is not stored, not that the principal is unrecoverable.
fn target_hash(domain: &str, value: &str) -> String {
    let normalized = value.trim().to_lowercase();
    let preimage = format!("assay.tool_target.v0:{domain}:{normalized}");
    format!("sha256:{}", sha256_hex(preimage.as_bytes()))
}

/// Argument keys whose values are secret-like. The classifiers never read these for projection and
/// never hash them (a hash of a public key can still leak correlation; a token hash invites offline
/// brute force). They are dropped by construction: the classifiers only read the allowlisted target
/// fields. This list and the detector below exist to TEST that discipline, not to drive it.
#[cfg(test)]
const SECRET_LIKE_KEYS: &[&str] = &[
    "public_key",
    "private_key",
    "token",
    "access_token",
    "authorization",
    "secret",
    "credential",
    "password",
    "api_key",
];

fn str_field<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str())
}

#[cfg(test)]
fn observed_secret_arg(args: &Value) -> bool {
    args.as_object()
        .map(|o| o.keys().any(|k| SECRET_LIKE_KEYS.contains(&k.as_str())))
        .unwrap_or(false)
}

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
    /// Raw call arguments. They are inspected by the classifier to PROJECT named target fields
    /// (hashing sensitive ids); they are never copied into the record verbatim.
    pub args: &'a Value,
    pub effect: Effect,
    /// Machine-readable status string (e.g. "success", "blocked", "error", "timeout").
    pub status: &'a str,
    /// Policy rule id if the decision named one; `None` falls back to a neutral marker.
    pub rule_id: Option<&'a str>,
}

/// Outcome of running the rule-based privileged-action classifiers over one observed call. Total:
/// every call yields one of the states below, never `None`. An unrecognized tool is still evidence.
pub struct Classified {
    /// Tool category (`github_deploy_key` etc.), or `None` for an unrecognized tool.
    pub category: Option<&'static str>,
    /// One of: `classified`, `classified_incomplete`, `observed_unknown_tool`, `redaction_failed`.
    pub state: &'static str,
    pub class: &'static str,
    pub verb: Option<&'static str>,
    pub resource_type: Option<&'static str>,
    /// Projected target: only named, allowlisted fields, sensitive ids hashed. `Null` when unknown.
    pub target: Value,
    /// Machine-readable reason for the state (downstream never parses prose).
    pub reason_code: &'static str,
    /// Optional human-readable specifics (e.g. which field was missing).
    pub detail: Option<String>,
}

fn unknown() -> Classified {
    Classified {
        category: None,
        state: "observed_unknown_tool",
        class: "unclassified",
        verb: None,
        resource_type: None,
        target: Value::Null,
        reason_code: "unknown_tool_name",
        detail: None,
    }
}

/// The scope a classified action requires, derived deterministically from the action category. This
/// is Assay's static claim about what the action needs, NOT a provider-verified grant requirement and
/// NOT inferred from arguments. `None` for an unclassified tool (the consumer reads that as
/// `required_scope_unknown`, never as "no scope needed"). A consumer compares this against the scopes
/// an operator declared for the credential alias (see docs/reference/credential-scope.md). The
/// enforcing proxy's credential-scope gate (P61e-c2) reads this as the per-action required scope.
pub fn required_scope_for(category: Option<&str>) -> Option<&'static str> {
    match category {
        Some("github_deploy_key") => Some("repo:deploy_key:write"),
        Some("slack_add_member") => Some("conversations:members:write"),
        Some("workspace_admin") => Some("workspace:admin"),
        _ => None,
    }
}

fn incomplete(
    category: &'static str,
    verb: &'static str,
    resource_type: &'static str,
    target: Value,
    detail: &str,
) -> Classified {
    Classified {
        category: Some(category),
        state: "classified_incomplete",
        class: "privileged_admin_action",
        verb: Some(verb),
        resource_type: Some(resource_type),
        target,
        reason_code: "missing_required_target_field",
        detail: Some(detail.to_string()),
    }
}

/// Rule-based privileged-action classifiers. Explicit name/alias matching only; no model or judge
/// decides a classification. The classifier reads args ONLY to project allowlisted target fields,
/// hashing sensitive ids under per-field domains; everything else (including any secret-like key) is
/// dropped, never copied. A matched tool with a missing required field is `classified_incomplete`
/// (never silently safe); an unmatched tool is `observed_unknown_tool` (never silently clean).
pub fn classify(tool_name: &str, args: &Value) -> Classified {
    let leaf = tool_name.rsplit('.').next().unwrap_or(tool_name);

    // github_deploy_key: owner + repo are required; owner/repo are plain (not sensitive), the key
    // title is hashed, the public/private key material is dropped (never read).
    if matches!(leaf, "add_deploy_key" | "create_deploy_key") {
        let owner = str_field(args, "owner");
        let repo = str_field(args, "repo");
        let mut target = json!({ "provider": "github" });
        if let Some(o) = owner {
            target["owner"] = json!(sanitize(o));
        }
        if let (Some(_), Some(r)) = (owner, repo) {
            target["repo"] = json!(sanitize(r));
            if let Some(title) = str_field(args, "title").or_else(|| str_field(args, "key_title")) {
                target["key_title_hash"] = json!(target_hash("github_key_title", title));
            }
            if let Some(ro) = args.get("read_only").and_then(|v| v.as_bool()) {
                target["read_only"] = json!(ro);
            }
            return Classified {
                category: Some("github_deploy_key"),
                state: "classified",
                class: "privileged_admin_action",
                verb: Some("create"),
                resource_type: Some("github_deploy_key"),
                target,
                reason_code: "classified_github_deploy_key",
                detail: None,
            };
        }
        return incomplete(
            "github_deploy_key",
            "create",
            "github_deploy_key",
            target,
            "missing_github_owner_or_repo",
        );
    }

    // slack_add_member: a scope (workspace and/or channel) plus a principal. All ids are hashed
    // under their own domains; channel is null for workspace-level membership.
    if matches!(leaf, "add_member" | "invite") {
        let workspace = str_field(args, "workspace_id");
        let channel = str_field(args, "channel_id");
        let principal = str_field(args, "user_id").or_else(|| str_field(args, "user"));
        if let (true, Some(p)) = (workspace.is_some() || channel.is_some(), principal) {
            let target = json!({
                "provider": "slack",
                "workspace_id_hash": workspace.map(|w| target_hash("slack_workspace", w)),
                "channel_id_hash": channel.map(|c| target_hash("slack_channel", c)),
                "principal_hash": target_hash("slack_principal", p),
            });
            return Classified {
                category: Some("slack_add_member"),
                state: "classified",
                class: "privileged_admin_action",
                verb: Some("add"),
                resource_type: Some("workspace_member"),
                target,
                reason_code: "classified_slack_add_member",
                detail: None,
            };
        }
        let detail = if principal.is_none() {
            "missing_slack_principal"
        } else {
            "missing_slack_scope"
        };
        return incomplete(
            "slack_add_member",
            "add",
            "workspace_member",
            json!({ "provider": "slack" }),
            detail,
        );
    }

    // workspace_admin: a deliberately narrow set of concrete admin verbs. workspace + principal are
    // hashed; the role is a plain label.
    let workspace_verb = match leaf {
        "grant_admin" => Some("grant"),
        "change_role" => Some("change_role"),
        "invite_external" => Some("invite"),
        "modify_org_policy" => Some("modify"),
        "create_workspace_token" => Some("create"),
        _ => None,
    };
    if let Some(verb) = workspace_verb {
        let workspace = str_field(args, "workspace_id")
            .or_else(|| str_field(args, "workspace"))
            .or_else(|| str_field(args, "org"));
        let principal = str_field(args, "principal").or_else(|| str_field(args, "user"));
        if let (Some(w), Some(p)) = (workspace, principal) {
            let mut target = json!({
                "provider": "workspace",
                "workspace_id_hash": target_hash("workspace", w),
                "principal_hash": target_hash("workspace_principal", p),
            });
            if let Some(role) = str_field(args, "role") {
                target["role"] = json!(sanitize(role));
            }
            return Classified {
                category: Some("workspace_admin"),
                state: "classified",
                class: "privileged_admin_action",
                verb: Some(verb),
                resource_type: Some("workspace_role"),
                target,
                reason_code: "classified_workspace_admin",
                detail: None,
            };
        }
        let detail = if workspace.is_none() {
            "missing_workspace_id"
        } else {
            "missing_workspace_principal"
        };
        return incomplete(
            "workspace_admin",
            verb,
            "workspace_role",
            json!({ "provider": "workspace" }),
            detail,
        );
    }

    unknown()
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
    let c = classify(call.tool_name, call.args);
    let mut decision = json!({
        "server": { "id": sanitize(call.server_id), "transport": "mcp" },
        "tool": {
            "name": sanitize(call.tool_name),
            "category": c.category.map(Value::from).unwrap_or(Value::Null)
        },
        "classification": c.state,
        // Machine-readable reason for the classification; downstream never parses prose.
        "reason_code": c.reason_code,
        "action": {
            "class": c.class,
            "verb": c.verb.map(Value::from).unwrap_or(Value::Null),
            "resource_type": c.resource_type.map(Value::from).unwrap_or(Value::Null),
            // The target carries only named, allowlisted fields the classifier projected (sensitive
            // ids hashed under per-field domains); never raw args, never secret material.
            "target": c.target,
            // Static scope this action requires (from the category, not the args). Null when
            // unclassified: the consumer reads that as required_scope_unknown, never "no scope".
            "required_scope": required_scope_for(c.category).map(Value::from).unwrap_or(Value::Null)
        },
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
    });
    if let Some(detail) = c.detail {
        decision["detail"] = Value::String(detail);
    }
    decision
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
mod tests;
