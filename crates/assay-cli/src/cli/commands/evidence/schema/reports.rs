use serde::Serialize;
use std::fmt;

pub(super) const SCHEMA_LIST_REPORT: &str = "assay.evidence.schema.list.v1";
pub(super) const SCHEMA_SHOW_REPORT: &str = "assay.evidence.schema.show.v1";
pub(super) const SCHEMA_VALIDATION_REPORT: &str = "assay.evidence.schema.validation.v1";

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SchemaKind {
    Receipt,
    Input,
}

impl fmt::Display for SchemaKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Receipt => f.write_str("receipt"),
            Self::Input => f.write_str("input"),
        }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct SchemaListReport {
    pub(super) schema: &'static str,
    pub(super) schemas: Vec<SchemaMetadata>,
}

#[derive(Debug, Serialize)]
pub(super) struct SchemaShowReport {
    pub(super) schema: &'static str,
    pub(super) metadata: SchemaMetadata,
}

#[derive(Debug, Serialize)]
pub(super) struct SchemaMetadata {
    pub(super) name: String,
    pub(super) kind: SchemaKind,
    pub(super) status: String,
    pub(super) family: String,
    pub(super) json_schema_id: String,
    pub(super) source_path: String,
    pub(super) description: String,
    pub(super) trust_basis_claim: Option<String>,
    pub(super) importer_only: bool,
    pub(super) aliases: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct SchemaValidationReport {
    pub(super) schema: &'static str,
    pub(super) schema_name: String,
    pub(super) schema_kind: SchemaKind,
    pub(super) input: String,
    pub(super) jsonl: bool,
    pub(super) valid: bool,
    pub(super) documents: usize,
    pub(super) errors: Vec<SchemaValidationError>,
}

impl SchemaValidationReport {
    pub(super) fn has_input_errors(&self) -> bool {
        self.errors.iter().any(|error| {
            matches!(
                error.kind,
                SchemaValidationErrorKind::Parse | SchemaValidationErrorKind::EmptyInput
            )
        })
    }
}

#[derive(Debug, Serialize)]
pub(super) struct SchemaValidationError {
    pub(super) document: usize,
    pub(super) kind: SchemaValidationErrorKind,
    pub(super) instance_path: String,
    pub(super) message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SchemaValidationErrorKind {
    Parse,
    EmptyInput,
    Schema,
}
