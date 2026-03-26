use super::super::{CheckContext, CheckResult};
use super::finding::{create_finding, create_finding_with_severity};
use super::json_path::value_pointer;
use crate::lint::packs::schema::{PackRule, Severity};

/// Check: manifest contains specified field.
pub(in crate::lint::packs::checks) fn check_manifest_field(
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
            };
        }
    };

    let has_field = value_pointer(&manifest_json, path).is_some();

    if has_field {
        CheckResult {
            passed: true,
            finding: None,
        }
    } else {
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
