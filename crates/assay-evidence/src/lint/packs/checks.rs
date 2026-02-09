//! Check implementations for pack rules.
//!
//! Each check type from SPEC-Pack-Engine-v1 has a corresponding implementation here.

use super::schema::{CheckDefinition, PackRule, Severity};
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
pub const ENGINE_VERSION: &str = "1.0";

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
        CheckDefinition::JsonPathExists { paths } => check_json_path_exists(rule, ctx, paths),
        CheckDefinition::Conditional { .. } => {
            handle_unsupported_check(rule, ctx, "Conditional checks require engine v1.1")
        }
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

/// Check: JSON path exists in events.
fn check_json_path_exists(
    rule: &PackRule,
    ctx: &CheckContext<'_>,
    paths: &[String],
) -> CheckResult {
    for event in ctx.events {
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
            format!("No event contains paths: {}", paths.join(", ")),
            None,
        )),
    }
}

/// Check: bundle contains minimum number of events.
fn check_event_count(rule: &PackRule, ctx: &CheckContext<'_>, min: usize) -> CheckResult {
    let count = ctx.events.len();
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
                format!("Bundle contains {} events (minimum: {})", count, min),
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

    let has_start = ctx.events.iter().any(|e| start_matcher.is_match(&e.type_));
    let has_finish = ctx.events.iter().any(|e| finish_matcher.is_match(&e.type_));

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
    for event in ctx.events {
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

    if ctx.events.iter().any(|e| matcher.is_match(&e.type_)) {
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
