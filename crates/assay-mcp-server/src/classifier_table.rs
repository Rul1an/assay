//! Private classifier inventory shared by the library and the binary test builds (#2232).
//!
//! One table, one meaning: [`classify`](crate::tool_decision::classify) resolves the tool-name
//! leaf through [`CLASSIFIER_TOOLS`], and every test matrix (the decision-record claims scan,
//! the annotation-conformance parity check, the reflected-field excision pin) iterates this
//! same table instead of a hand list — so a new category, leaf, or verb joins every scan the
//! moment it is added here, never via a second inventory free to drift from this one.
//!
//! This file is compiled into both test builds — the library's via `mod classifier_table;`
//! in `lib.rs` and the binary's via the same declaration in `main.rs` — so both targets
//! iterate this same table over these same matrix constructors. The module is private in both,
//! and every item is `pub(crate)` at most, so sharing it adds no public API (the same
//! discipline as `claims_backstop.rs`).
//!
//! [`CLASSIFIER_TOOLS`] is production vocabulary (`classify` reads it), so it is always
//! compiled. The `classifier_*_args` constructors and [`REFLECTED_TARGET_FIELDS`] are
//! test-matrix-only — production never calls them — so they are `#[cfg(test)]`-gated and absent
//! from release builds.

#[cfg(test)]
use serde_json::{json, Value};

/// One classifiable tool leaf: the canonical inventory of what `classify` can emit.
pub(crate) struct ClassifierTool {
    /// Tool-name leaf after the last `.` (e.g. `add_deploy_key` in `github.add_deploy_key`).
    pub(crate) leaf: &'static str,
    pub(crate) category: &'static str,
    pub(crate) verb: &'static str,
    pub(crate) resource_type: &'static str,
}

/// Every (leaf, category, verb, resource_type) the classifier can emit, one row per leaf.
pub(crate) const CLASSIFIER_TOOLS: &[ClassifierTool] = &[
    ClassifierTool {
        leaf: "add_deploy_key",
        category: "github_deploy_key",
        verb: "create",
        resource_type: "github_deploy_key",
    },
    ClassifierTool {
        leaf: "create_deploy_key",
        category: "github_deploy_key",
        verb: "create",
        resource_type: "github_deploy_key",
    },
    ClassifierTool {
        leaf: "add_member",
        category: "slack_add_member",
        verb: "add",
        resource_type: "workspace_member",
    },
    ClassifierTool {
        leaf: "invite",
        category: "slack_add_member",
        verb: "add",
        resource_type: "workspace_member",
    },
    ClassifierTool {
        leaf: "grant_admin",
        category: "workspace_admin",
        verb: "grant",
        resource_type: "workspace_role",
    },
    ClassifierTool {
        leaf: "change_role",
        category: "workspace_admin",
        verb: "change_role",
        resource_type: "workspace_role",
    },
    ClassifierTool {
        leaf: "invite_external",
        category: "workspace_admin",
        verb: "invite",
        resource_type: "workspace_role",
    },
    ClassifierTool {
        leaf: "modify_org_policy",
        category: "workspace_admin",
        verb: "modify",
        resource_type: "workspace_role",
    },
    ClassifierTool {
        leaf: "create_workspace_token",
        category: "workspace_admin",
        verb: "create",
        resource_type: "workspace_role",
    },
];

/// Complete (classifiable) arguments for one classifier category: the matrix input that must
/// reach `classified`. A test-matrix constructor — production never calls this; it exists so the
/// scan inputs derive from the category vocabulary above instead of a hand list per test.
#[cfg(test)]
pub(crate) fn classifier_complete_args(category: &str) -> Value {
    match category {
        "github_deploy_key" => json!({"owner": "acme", "repo": "prod-app"}),
        "slack_add_member" => json!({"workspace_id": "acme", "user_id": "u1"}),
        "workspace_admin" => json!({"workspace_id": "acme", "principal": "p"}),
        _ => Value::Null,
    }
}

/// Complete arguments WITH every optional projected field for the category: the matrix
/// input that reaches `classified` while also emitting the optional target leaves — the title
/// hash and `read_only` on github, a non-null channel hash on slack, `role` on workspace.
/// The plain complete input leaves those leaves absent, so a mutant on an optional path
/// survives any scan that only runs the plain input.
#[cfg(test)]
pub(crate) fn classifier_complete_args_with_optionals(category: &str) -> Value {
    match category {
        // `key_title` (not `title`) exercises the `title.or(key_title)` fallback arm, so a
        // mutant on the `key_title` path is reached here and not only in the `title` shape test.
        "github_deploy_key" => {
            json!({"owner": "acme", "repo": "prod-app", "key_title": "ci-key", "read_only": true})
        }
        "slack_add_member" => {
            json!({"workspace_id": "acme", "channel_id": "C1", "user_id": "u1"})
        }
        "workspace_admin" => {
            json!({"workspace_id": "acme", "principal": "p", "role": "admin"})
        }
        _ => Value::Null,
    }
}

/// Arguments missing one required field for the category: the matrix input that must reach
/// `classified_incomplete`. Same discipline as `classifier_complete_args`.
#[cfg(test)]
pub(crate) fn classifier_incomplete_args(category: &str) -> Value {
    match category {
        "github_deploy_key" => json!({"owner": "acme"}),
        "slack_add_member" => json!({"workspace_id": "acme"}),
        "workspace_admin" => json!({"workspace_id": "acme"}),
        _ => json!({}),
    }
}

/// Target sub-leaves that carry caller bytes verbatim (sanitized, never hashed): `owner`/`repo`
/// from the github projection and `role` from the workspace projection. Claims guards excise
/// exactly these leaves before word scans and pin them byte-exact instead; everything else under
/// `target` (provider labels, `*_hash` digests, booleans, and the key names themselves) is
/// Assay-authored and stays scanned.
#[cfg(test)]
pub(crate) const REFLECTED_TARGET_FIELDS: &[&str] = &["owner", "repo", "role"];
