//! Check implementations for pack rules.
//!
//! Each check type from SPEC-Pack-Engine-v1 has a corresponding implementation here.

use super::schema::{CheckDefinition, PackRule, Severity, SupportedConditionalCheck};
use crate::bundle::writer::Manifest;
use crate::lint::{EventLocation, LintFinding};
use crate::types::EvidenceEvent;
use globset::{Glob, GlobMatcher};
use sha2::Digest;

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

/// G3 v1: same predicate as Trust Basis `authorization_context_visible` (verified).
///
/// Honors `rule.event_types` like other checks: only scoped events are considered.
fn check_g3_authorization_context_present(rule: &PackRule, ctx: &CheckContext<'_>) -> CheckResult {
    let passed = scoped_events(rule, ctx).into_iter().any(|event| {
        crate::g3_authorization_context::decision_event_satisfies_g3_authorization_context_visible(
            event,
        )
    });
    if passed {
        CheckResult {
            passed: true,
            finding: None,
        }
    } else {
        CheckResult {
            passed: false,
            finding: Some(create_finding(
                rule,
                ctx,
                "No assay.tool.decision event satisfies G3 v1 policy-projected authorization context (principal + allowlisted auth_scheme + auth_issuer with G3 string discipline).".to_string(),
                None,
            )),
        }
    }
}

/// Check: JSON path exists in events (optional value equality at each path).
fn check_json_path_exists(
    rule: &PackRule,
    ctx: &CheckContext<'_>,
    paths: &[String],
    value_equals: Option<&serde_json::Value>,
) -> CheckResult {
    for event in scoped_events(rule, ctx) {
        let json = match serde_json::to_value(event) {
            Ok(v) => v,
            Err(_) => continue,
        };

        for path in paths {
            match (value_pointer(&json, path), value_equals) {
                (Some(v), Some(required)) if v == required => {
                    return CheckResult {
                        passed: true,
                        finding: None,
                    };
                }
                (Some(_), Some(_)) => continue,
                (Some(_), None) => {
                    return CheckResult {
                        passed: true,
                        finding: None,
                    };
                }
                (None, _) => continue,
            }
        }
    }

    let detail = match value_equals {
        Some(req) => format!("No event has path {} equal to {}", paths.join(", "), req),
        None => format!("No event contains paths: {}", paths.join(", ")),
    };
    CheckResult {
        passed: false,
        finding: Some(create_finding(rule, ctx, detail, None)),
    }
}

/// Check: bundle contains minimum number of events.
fn check_event_count(rule: &PackRule, ctx: &CheckContext<'_>, min: usize) -> CheckResult {
    let count = scoped_events(rule, ctx).len();
    if count >= min {
        CheckResult {
            passed: true,
            finding: None,
        }
    } else {
        CheckResult {
            passed: false,
            finding: Some(create_finding(
                rule,
                ctx,
                scoped_event_count_message(rule, count, min),
                None,
            )),
        }
    }
}

/// Check: matching start/finish event pairs exist.
fn check_event_pairs(
    rule: &PackRule,
    ctx: &CheckContext<'_>,
    start_pattern: &str,
    finish_pattern: &str,
) -> CheckResult {
    let start_matcher = match compile_glob(start_pattern) {
        Some(m) => m,
        None => {
            return CheckResult {
                passed: false,
                finding: Some(create_finding(
                    rule,
                    ctx,
                    format!("Invalid start pattern: {}", start_pattern),
                    None,
                )),
            }
        }
    };

    let finish_matcher = match compile_glob(finish_pattern) {
        Some(m) => m,
        None => {
            return CheckResult {
                passed: false,
                finding: Some(create_finding(
                    rule,
                    ctx,
                    format!("Invalid finish pattern: {}", finish_pattern),
                    None,
                )),
            }
        }
    };

    let scoped_events = scoped_events(rule, ctx);
    let has_start = scoped_events
        .iter()
        .any(|e| start_matcher.is_match(&e.type_));
    let has_finish = scoped_events
        .iter()
        .any(|e| finish_matcher.is_match(&e.type_));

    if has_start && has_finish {
        CheckResult {
            passed: true,
            finding: None,
        }
    } else {
        let missing = match (has_start, has_finish) {
            (false, false) => format!(
                "Missing both start ({}) and finish ({}) events",
                start_pattern, finish_pattern
            ),
            (false, true) => format!("Missing start event matching '{}'", start_pattern),
            (true, false) => format!("Missing finish event matching '{}'", finish_pattern),
            _ => unreachable!(),
        };
        CheckResult {
            passed: false,
            finding: Some(create_finding(rule, ctx, missing, None)),
        }
    }
}

/// Check: at least one event contains specified fields (JSON Pointer paths).
fn check_event_field_present(
    rule: &PackRule,
    ctx: &CheckContext<'_>,
    paths: &[String],
) -> CheckResult {
    for event in scoped_events(rule, ctx) {
        // Serialize once per event, then check all paths
        let json = match serde_json::to_value(event) {
            Ok(v) => v,
            Err(_) => continue,
        };

        for path in paths {
            if value_pointer(&json, path).is_some() {
                return CheckResult {
                    passed: true,
                    finding: None,
                };
            }
        }
    }

    CheckResult {
        passed: false,
        finding: Some(create_finding(
            rule,
            ctx,
            format!(
                "No event contains any of the required fields: {}",
                paths.join(", ")
            ),
            None,
        )),
    }
}

/// Check: at least one event of specified type exists.
fn check_event_type_exists(rule: &PackRule, ctx: &CheckContext<'_>, pattern: &str) -> CheckResult {
    let matcher = match compile_glob(pattern) {
        Some(m) => m,
        None => {
            return CheckResult {
                passed: false,
                finding: Some(create_finding(
                    rule,
                    ctx,
                    format!("Invalid pattern: {}", pattern),
                    None,
                )),
            }
        }
    };

    if scoped_events(rule, ctx)
        .into_iter()
        .any(|e| matcher.is_match(&e.type_))
    {
        CheckResult {
            passed: true,
            finding: None,
        }
    } else {
        CheckResult {
            passed: false,
            finding: Some(create_finding(
                rule,
                ctx,
                format!("No event found matching type pattern '{}'", pattern),
                None,
            )),
        }
    }
}

/// Check: if matching events satisfy a narrow condition, required path must
/// exist on the same event.
fn check_conditional(
    rule: &PackRule,
    ctx: &CheckContext<'_>,
    conditional: &SupportedConditionalCheck,
) -> CheckResult {
    let mut matched_events = 0usize;
    let mut missing_required_path = 0usize;
    let mut first_missing_event: Option<&EvidenceEvent> = None;

    for event in scoped_events(rule, ctx) {
        let json = match serde_json::to_value(event) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if !conditional
            .clauses
            .iter()
            .all(|clause| value_pointer(&json, &clause.path) == Some(&clause.equals))
        {
            continue;
        }

        matched_events += 1;

        if value_pointer(&json, &conditional.required_path).is_none() {
            missing_required_path += 1;
            if first_missing_event.is_none() {
                first_missing_event = Some(event);
            }
        }
    }

    if matched_events == 0 || missing_required_path == 0 {
        return CheckResult {
            passed: true,
            finding: None,
        };
    }

    let matched_label = if matched_events == 1 {
        "event"
    } else {
        "events"
    };
    let missing_label = if missing_required_path == 1 {
        "matching event was"
    } else {
        "matching events were"
    };

    CheckResult {
        passed: false,
        finding: Some(create_finding(
            rule,
            ctx,
            format!(
                "{} {} matched the condition, but {} {} missing required path: {}",
                matched_events,
                matched_label,
                missing_required_path,
                missing_label,
                conditional.required_path
            ),
            first_missing_event.map(event_location),
        )),
    }
}

/// Check: manifest contains specified field.
fn check_manifest_field(
    rule: &PackRule,
    ctx: &CheckContext<'_>,
    path: &str,
    required: bool,
) -> CheckResult {
    let manifest_json = match serde_json::to_value(ctx.manifest) {
        Ok(v) => v,
        Err(_) => {
            return CheckResult {
                passed: false,
                finding: Some(create_finding(
                    rule,
                    ctx,
                    "Failed to serialize manifest".to_string(),
                    None,
                )),
            }
        }
    };

    let has_field = value_pointer(&manifest_json, path).is_some();

    if has_field {
        CheckResult {
            passed: true,
            finding: None,
        }
    } else {
        // If not required, this is a warning-level finding
        let severity = if required {
            rule.severity
        } else {
            Severity::Warn
        };

        CheckResult {
            passed: !required,
            finding: Some(create_finding_with_severity(
                rule,
                ctx,
                format!("Manifest missing field: {}", path),
                None,
                severity,
            )),
        }
    }
}

/// Create a lint finding for a pack rule.
fn create_finding(
    rule: &PackRule,
    ctx: &CheckContext<'_>,
    message: String,
    location: Option<EventLocation>,
) -> LintFinding {
    create_finding_with_severity(rule, ctx, message, location, rule.severity)
}

/// Create a lint finding with explicit severity.
fn create_finding_with_severity(
    rule: &PackRule,
    ctx: &CheckContext<'_>,
    message: String,
    location: Option<EventLocation>,
    severity: Severity,
) -> LintFinding {
    let canonical_id = format!("{}@{}:{}", ctx.pack_name, ctx.pack_version, rule.id);

    // Compute fingerprint for GitHub dedup
    let location_key = match &location {
        Some(loc) => format!("{}:{}", loc.seq, loc.line),
        None => "global".into(),
    };

    let fingerprint = format!(
        "sha256:{}",
        hex::encode(sha2::Sha256::digest(
            format!("{}:{}:{}", canonical_id, location_key, ctx.pack_digest).as_bytes()
        ))
    );

    // Compute primaryLocationLineHash for GitHub
    let start_line = location.as_ref().map(|l| l.line).unwrap_or(1);
    let artifact_uri = location
        .as_ref()
        .map(|_| "events.ndjson")
        .unwrap_or(ctx.bundle_path);

    let primary_hash = hex::encode(sha2::Sha256::digest(
        format!(
            "{}:{}:{}:{}",
            canonical_id, artifact_uri, start_line, ctx.pack_digest
        )
        .as_bytes(),
    ));

    LintFinding {
        rule_id: canonical_id,
        severity,
        message,
        location,
        fingerprint,
        help_uri: None,
        tags: vec![ctx.pack_name.to_string(), format!("pack:{}", ctx.pack_name)],
        // Pack-specific fields (stored in extended data)
        // These will be used in SARIF output
    }
    .with_pack_metadata(
        ctx.pack_name,
        ctx.pack_version,
        &rule.id,
        rule.article_ref.as_deref(),
        &primary_hash,
    )
}

/// Compile a glob pattern.
fn compile_glob(pattern: &str) -> Option<GlobMatcher> {
    Glob::new(pattern).ok().map(|g| g.compile_matcher())
}

fn scoped_events<'a>(rule: &PackRule, ctx: &'a CheckContext<'a>) -> Vec<&'a EvidenceEvent> {
    match &rule.event_types {
        Some(event_types) if !event_types.is_empty() => ctx
            .events
            .iter()
            .filter(|event| event_types.iter().any(|expected| expected == &event.type_))
            .collect(),
        _ => ctx.events.iter().collect(),
    }
}

fn scoped_event_count_message(rule: &PackRule, count: usize, min: usize) -> String {
    match &rule.event_types {
        Some(event_types) if !event_types.is_empty() => format!(
            "Scoped events for event_types [{}] contain {} events (minimum: {})",
            event_types.join(", "),
            count,
            min
        ),
        _ => format!("Bundle contains {} events (minimum: {})", count, min),
    }
}

fn event_location(event: &EvidenceEvent) -> EventLocation {
    EventLocation {
        seq: event.seq as usize,
        line: event.seq as usize + 1,
        event_type: Some(event.type_.clone()),
    }
}

fn value_pointer<'a>(value: &'a serde_json::Value, pointer: &str) -> Option<&'a serde_json::Value> {
    if pointer.is_empty() || pointer == "/" {
        return Some(value);
    }
    let normalized;
    let pointer = if pointer.starts_with('/') {
        pointer
    } else {
        normalized = format!("/{}", pointer);
        normalized.as_str()
    };
    value.pointer(pointer)
}

// Extension trait to add pack metadata to LintFinding
trait LintFindingExt {
    fn with_pack_metadata(
        self,
        pack_name: &str,
        pack_version: &str,
        short_id: &str,
        article_ref: Option<&str>,
        primary_hash: &str,
    ) -> Self;
}

impl LintFindingExt for LintFinding {
    fn with_pack_metadata(
        mut self,
        pack_name: &str,
        pack_version: &str,
        short_id: &str,
        article_ref: Option<&str>,
        primary_hash: &str,
    ) -> Self {
        // Store metadata in tags for now (will be extracted in SARIF output)
        self.tags.push(format!("pack_version:{}", pack_version));
        self.tags.push(format!("short_id:{}", short_id));
        if let Some(ref_) = article_ref {
            self.tags.push(format!("article_ref:{}", ref_));
        }
        self.tags
            .push(format!("primaryLocationLineHash:{}", primary_hash));
        let _ = pack_name; // Used in canonical ID already
        self
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

        assert!(value_pointer(&json, "/run_id").is_some());
        assert!(value_pointer(&json, "run_id").is_some());
        assert!(value_pointer(&json, "/data/traceparent").is_some());
        assert!(value_pointer(&json, "/data/nested/deep").is_some());
        assert!(value_pointer(&json, "/missing").is_none());
        assert!(value_pointer(&json, "/data/missing").is_none());
    }

    #[test]
    fn test_glob_matching() {
        let matcher = compile_glob("*.started").unwrap();
        assert!(matcher.is_match("assay.run.started"));
        assert!(matcher.is_match("mcp.tool.started"));
        assert!(!matcher.is_match("assay.run.finished"));
    }
}
