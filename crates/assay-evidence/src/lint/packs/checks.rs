//! Check implementations for pack rules.
//!
//! Each check type from SPEC-Pack-Engine-v1 has a corresponding implementation here.

use super::schema::{CheckDefinition, PackRule};
use crate::bundle::writer::Manifest;
use crate::lint::LintFinding;
use crate::types::EvidenceEvent;

#[path = "checks_next/mod.rs"]
mod checks_next;

use checks_next::{
    check_conditional, check_event_count, check_event_field_present, check_event_pairs,
    check_event_type_exists, check_g3_authorization_context_present, check_json_path_exists,
    check_manifest_field, create_finding,
};

/// Context for running pack checks.
pub struct CheckContext<'a> {
    /// All events in the bundle.
    pub events: &'a [EvidenceEvent],
    /// Bundle manifest.
    pub manifest: &'a Manifest,
    /// Bundle file path (for SARIF locations).
    pub bundle_path: &'a str,
    /// Pack name.
    pub pack_name: &'a str,
    /// Pack version.
    pub pack_version: &'a str,
    /// Pack digest.
    pub pack_digest: &'a str,
    /// Pack kind (compliance/security/quality).
    pub pack_kind: super::schema::PackKind,
}

/// Result of a check execution.
pub struct CheckResult {
    /// Whether the check passed.
    pub passed: bool,
    /// Finding if check failed.
    pub finding: Option<LintFinding>,
}

/// Current pack engine version.
pub const ENGINE_VERSION: &str = "1.2";

/// Execute a pack rule check.
///
/// For unsupported check types, behavior depends on `pack_kind`:
/// - Compliance packs: unsupported check = error (no compliance theater)
/// - Security/Quality: unsupported check = warning + skip
pub fn execute_check(rule: &PackRule, ctx: &CheckContext<'_>) -> CheckResult {
    // Check engine version requirement if specified
    if let Some(required_version) = &rule.engine_min_version {
        if !engine_version_satisfies(ENGINE_VERSION, required_version) {
            return handle_unsupported_check(
                rule,
                ctx,
                &format!(
                    "Rule requires engine v{}, current is v{}",
                    required_version, ENGINE_VERSION
                ),
            );
        }
    }

    match &rule.check {
        CheckDefinition::EventCount { min } => check_event_count(rule, ctx, *min),
        CheckDefinition::EventPairs {
            start_pattern,
            finish_pattern,
        } => check_event_pairs(rule, ctx, start_pattern, finish_pattern),
        CheckDefinition::EventFieldPresent { .. } => {
            let paths = rule.check.get_field_paths();
            check_event_field_present(rule, ctx, &paths)
        }
        CheckDefinition::EventTypeExists { pattern } => check_event_type_exists(rule, ctx, pattern),
        CheckDefinition::ManifestField { path, required } => {
            check_manifest_field(rule, ctx, path, *required)
        }
        CheckDefinition::JsonPathExists {
            paths,
            value_equals,
        } => check_json_path_exists(rule, ctx, paths, value_equals.as_ref()),
        CheckDefinition::G3AuthorizationContextPresent => {
            check_g3_authorization_context_present(rule, ctx)
        }
        CheckDefinition::Conditional { .. } => match rule.check.supported_conditional() {
            Ok(conditional) => check_conditional(rule, ctx, &conditional),
            Err(reason) => handle_unsupported_check(
                rule,
                ctx,
                &format!("Unsupported conditional shape for engine v1.1: {reason}"),
            ),
        },
        CheckDefinition::Unsupported => handle_unsupported_check(
            rule,
            ctx,
            &format!("Unknown check type '{}'", rule.check.type_name()),
        ),
    }
}

/// Handle unsupported check types based on pack kind.
fn handle_unsupported_check(rule: &PackRule, ctx: &CheckContext<'_>, reason: &str) -> CheckResult {
    // For compliance packs: unsupported = hard fail (no compliance theater)
    // For security/quality: skip with warning
    use super::schema::PackKind;

    if ctx.pack_kind == PackKind::Compliance {
        CheckResult {
            passed: false,
            finding: Some(create_finding(
                rule,
                ctx,
                format!(
                    "Cannot execute rule: {}. Compliance packs require all rules to be executable.",
                    reason
                ),
                None,
            )),
        }
    } else {
        tracing::warn!(
            rule_id = %rule.id,
            pack = %ctx.pack_name,
            reason = %reason,
            "Skipping unsupported check"
        );
        CheckResult {
            passed: true, // Skip = pass (but logged)
            finding: None,
        }
    }
}

/// Check if current engine version satisfies requirement.
fn engine_version_satisfies(current: &str, required: &str) -> bool {
    // Parse simple semver (major.minor)
    let parse = |v: &str| -> Option<(u32, u32)> {
        let parts: Vec<&str> = v.split('.').collect();
        Some((parts.first()?.parse().ok()?, parts.get(1)?.parse().ok()?))
    };

    match (parse(current), parse(required)) {
        (Some((c_major, c_minor)), Some((r_major, r_minor))) => {
            (c_major, c_minor) >= (r_major, r_minor)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ProducerMeta;
    use chrono::{TimeZone, Utc};
    use serde_json::json;
    use std::collections::BTreeMap;

    use crate::lint::packs::schema::{CheckDefinition, PackKind, PackRule};
    use crate::lint::Severity;
    use crate::types::EvidenceEvent;

    fn mk_g3_decision(seq: u64) -> EvidenceEvent {
        let mut e = EvidenceEvent::new(
            "assay.tool.decision",
            "urn:assay:test",
            "run1",
            seq,
            json!({
                "tool": "t",
                "decision": "allow",
                "principal": "alice@example.com",
                "auth_scheme": "jwt_bearer",
                "auth_issuer": "https://issuer.example/"
            }),
        );
        e.time = Utc.timestamp_opt(1_700_000_000 + seq as i64, 0).unwrap();
        e
    }

    fn mk_fs_event(seq: u64) -> EvidenceEvent {
        let mut e = EvidenceEvent::new(
            "assay.fs.access",
            "urn:assay:test",
            "run1",
            seq,
            json!({ "path": "/tmp/x" }),
        );
        e.time = Utc.timestamp_opt(1_700_000_000 + seq as i64, 0).unwrap();
        e
    }

    fn g3_test_rule(event_types: Option<Vec<String>>) -> PackRule {
        PackRule {
            id: "T-G3".into(),
            severity: Severity::Warn,
            description: "test".into(),
            article_ref: None,
            help_markdown: None,
            check: CheckDefinition::G3AuthorizationContextPresent,
            engine_min_version: Some("1.2".into()),
            event_types,
        }
    }

    fn g3_test_ctx<'a>(manifest: &'a Manifest, events: &'a [EvidenceEvent]) -> CheckContext<'a> {
        CheckContext {
            events,
            manifest,
            bundle_path: "t.tar.gz",
            pack_name: "p",
            pack_version: "1",
            pack_digest: "d",
            pack_kind: PackKind::Security,
        }
    }

    #[test]
    fn g3_authorization_check_uses_scoped_events_not_full_bundle() {
        // G3-good decision + unrelated FS event; narrowing scope to FS only must not see the decision.
        let events = vec![mk_g3_decision(0), mk_fs_event(1)];
        let manifest = Manifest {
            schema_version: 1,
            bundle_id: "b".into(),
            producer: ProducerMeta::new("test", "0"),
            run_id: "r".into(),
            event_count: events.len(),
            run_root: "".into(),
            algorithms: Default::default(),
            files: BTreeMap::new(),
        };
        let ctx = g3_test_ctx(&manifest, &events);

        let mut rule = g3_test_rule(Some(vec!["assay.fs.access".into()]));
        let r = execute_check(&rule, &ctx);
        assert!(
            !r.passed,
            "scoped to fs only: no G3 on assay.fs.access, must fail"
        );

        rule.event_types = Some(vec!["assay.tool.decision".into()]);
        let r = execute_check(&rule, &ctx);
        assert!(
            r.passed,
            "scoped to tool.decision: G3 event in scope, must pass"
        );

        rule.event_types = None;
        let r = execute_check(&rule, &ctx);
        assert!(r.passed, "unscoped: G3 satisfied somewhere in bundle");
    }

    #[test]
    fn test_value_pointer() {
        let json: serde_json::Value = serde_json::json!({
            "run_id": "abc123",
            "data": {
                "traceparent": "00-...",
                "nested": {
                    "deep": "value"
                }
            }
        });

        assert!(checks_next::json_path::value_pointer(&json, "/run_id").is_some());
        assert!(checks_next::json_path::value_pointer(&json, "run_id").is_some());
        assert!(checks_next::json_path::value_pointer(&json, "/data/traceparent").is_some());
        assert!(checks_next::json_path::value_pointer(&json, "/data/nested/deep").is_some());
        assert!(checks_next::json_path::value_pointer(&json, "/missing").is_none());
        assert!(checks_next::json_path::value_pointer(&json, "/data/missing").is_none());
    }

    #[test]
    fn test_glob_matching() {
        let matcher = checks_next::event::compile_glob("*.started").unwrap();
        assert!(matcher.is_match("assay.run.started"));
        assert!(matcher.is_match("mcp.tool.started"));
        assert!(!matcher.is_match("assay.run.finished"));
    }
}
