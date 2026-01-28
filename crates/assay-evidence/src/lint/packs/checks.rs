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
}

/// Result of a check execution.
pub struct CheckResult {
    /// Whether the check passed.
    pub passed: bool,
    /// Finding if check failed.
    pub finding: Option<LintFinding>,
}

/// Execute a pack rule check.
pub fn execute_check(rule: &PackRule, ctx: &CheckContext<'_>) -> CheckResult {
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
        for path in paths {
            if event_has_field(event, path) {
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

    let has_field = json_pointer_get(&manifest_json, path).is_some();

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
            Severity::Warning
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
        severity: convert_severity(severity),
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

/// Convert pack severity to lint severity.
fn convert_severity(severity: Severity) -> crate::lint::Severity {
    match severity {
        Severity::Error => crate::lint::Severity::Error,
        Severity::Warning => crate::lint::Severity::Warn,
        Severity::Info => crate::lint::Severity::Info,
    }
}

/// Compile a glob pattern.
fn compile_glob(pattern: &str) -> Option<GlobMatcher> {
    Glob::new(pattern).ok().map(|g| g.compile_matcher())
}

/// Check if an event has a field at the given JSON Pointer path.
fn event_has_field(event: &EvidenceEvent, path: &str) -> bool {
    let json = match serde_json::to_value(event) {
        Ok(v) => v,
        Err(_) => return false,
    };

    json_pointer_get(&json, path).is_some()
}

/// Get a value from JSON using a JSON Pointer path (RFC 6901).
fn json_pointer_get<'a>(
    value: &'a serde_json::Value,
    pointer: &str,
) -> Option<&'a serde_json::Value> {
    if pointer.is_empty() || pointer == "/" {
        return Some(value);
    }

    let path = if pointer.starts_with('/') {
        &pointer[1..]
    } else {
        pointer
    };

    let mut current = value;
    for part in path.split('/') {
        // Unescape JSON Pointer escapes
        let unescaped = part.replace("~1", "/").replace("~0", "~");

        current = match current {
            serde_json::Value::Object(map) => map.get(&unescaped)?,
            serde_json::Value::Array(arr) => {
                let idx: usize = unescaped.parse().ok()?;
                arr.get(idx)?
            }
            _ => return None,
        };
    }

    Some(current)
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
    fn test_json_pointer_get() {
        let json: serde_json::Value = serde_json::json!({
            "run_id": "abc123",
            "data": {
                "traceparent": "00-...",
                "nested": {
                    "deep": "value"
                }
            }
        });

        assert!(json_pointer_get(&json, "/run_id").is_some());
        assert!(json_pointer_get(&json, "/data/traceparent").is_some());
        assert!(json_pointer_get(&json, "/data/nested/deep").is_some());
        assert!(json_pointer_get(&json, "/missing").is_none());
        assert!(json_pointer_get(&json, "/data/missing").is_none());
    }

    #[test]
    fn test_glob_matching() {
        let matcher = compile_glob("*.started").unwrap();
        assert!(matcher.is_match("assay.run.started"));
        assert!(matcher.is_match("mcp.tool.started"));
        assert!(!matcher.is_match("assay.run.finished"));
    }
}
