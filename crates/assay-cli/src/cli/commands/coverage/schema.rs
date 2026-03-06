#![allow(dead_code)]

use std::sync::OnceLock;

use anyhow::{anyhow, Result};
use jsonschema::Draft;
use serde_json::Value;

const COVERAGE_REPORT_V1_SCHEMA_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/schemas/coverage_report_v1.schema.json"
));

static VALIDATOR: OnceLock<jsonschema::Validator> = OnceLock::new();
static VALIDATOR_RESULT: OnceLock<Result<jsonschema::Validator, String>> = OnceLock::new();

fn compiled_validator() -> Result<&'static jsonschema::Validator> {
    let init = VALIDATOR_RESULT
        .get_or_init(|| {
            let schema: Value =
                serde_json::from_str(COVERAGE_REPORT_V1_SCHEMA_JSON).map_err(|e| {
                    format!("failed to parse embedded coverage_report_v1 schema JSON: {e}")
                })?;

            jsonschema::options()
                .with_draft(Draft::Draft202012)
                .build(&schema)
                .map_err(|e| format!("failed to compile coverage_report_v1 schema: {e}"))
        })
        .as_ref()
        .map_err(|e| anyhow!("{e}"))?;

    Ok(VALIDATOR.get_or_init(|| init.clone()))
}

pub fn validate_coverage_report_v1(instance: &Value) -> Result<()> {
    let validator = compiled_validator()?;
    if validator.is_valid(instance) {
        return Ok(());
    }

    const MAX_ERRORS: usize = 10;
    let mut lines = Vec::new();
    for (i, e) in validator.iter_errors(instance).take(MAX_ERRORS).enumerate() {
        lines.push(format!("{:02}: {}", i + 1, e));
    }

    let mut msg = String::from("coverage_report_v1 schema validation failed");
    if !lines.is_empty() {
        msg.push_str(":\n");
        msg.push_str(&lines.join("\n"));
    }
    Err(anyhow!(msg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn schema_compiles() {
        let _ = compiled_validator().expect("schema should compile");
    }

    #[test]
    fn empty_object_is_invalid() {
        let v = json!({});
        assert!(validate_coverage_report_v1(&v).is_err());
    }
}
