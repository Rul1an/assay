use super::registry::SchemaDescriptor;
use super::reports::{
    SchemaValidationError, SchemaValidationErrorKind, SchemaValidationReport,
    SCHEMA_VALIDATION_REPORT,
};
use serde_json::Value;
use std::path::Path;

pub(super) fn validate_input(
    descriptor: &SchemaDescriptor,
    input_path: &Path,
    jsonl: bool,
    input: &str,
    validator: &jsonschema::Validator,
) -> SchemaValidationReport {
    let mut errors = Vec::new();
    let mut documents = 0usize;

    if jsonl {
        for (idx, line) in input.lines().enumerate() {
            let line_number = idx + 1;
            if line.trim().is_empty() {
                continue;
            }
            documents += 1;
            match serde_json::from_str::<Value>(line) {
                Ok(value) => collect_validation_errors(line_number, &value, validator, &mut errors),
                Err(err) => errors.push(SchemaValidationError {
                    document: line_number,
                    kind: SchemaValidationErrorKind::Parse,
                    instance_path: "$".to_string(),
                    message: format!("invalid JSON: {err}"),
                }),
            }
        }
    } else {
        documents = 1;
        match serde_json::from_str::<Value>(input) {
            Ok(value) => collect_validation_errors(1, &value, validator, &mut errors),
            Err(err) => errors.push(SchemaValidationError {
                document: 1,
                kind: SchemaValidationErrorKind::Parse,
                instance_path: "$".to_string(),
                message: format!("invalid JSON: {err}"),
            }),
        }
    }

    if documents == 0 {
        errors.push(SchemaValidationError {
            document: 0,
            kind: SchemaValidationErrorKind::EmptyInput,
            instance_path: "$".to_string(),
            message: "no JSON documents found".to_string(),
        });
    }

    SchemaValidationReport {
        schema: SCHEMA_VALIDATION_REPORT,
        schema_name: descriptor.name.to_string(),
        schema_kind: descriptor.kind,
        input: input_path.display().to_string(),
        jsonl,
        valid: errors.is_empty(),
        documents,
        errors,
    }
}

fn collect_validation_errors(
    document: usize,
    value: &Value,
    validator: &jsonschema::Validator,
    errors: &mut Vec<SchemaValidationError>,
) {
    errors.extend(
        validator
            .iter_errors(value)
            .map(|err| SchemaValidationError {
                document,
                kind: SchemaValidationErrorKind::Schema,
                instance_path: err.instance_path().to_string(),
                message: err.to_string(),
            }),
    );
}
