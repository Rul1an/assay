use crate::cli::args::OutputFormat;
use crate::exit_codes::{EXIT_SUCCESS, EXIT_TEST_FAILURE};
use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};
use jsonschema::Draft;
use serde::Serialize;
use serde_json::Value;
use std::fmt;
use std::path::PathBuf;

const SCHEMA_LIST_REPORT: &str = "assay.evidence.schema.list.v1";
const SCHEMA_SHOW_REPORT: &str = "assay.evidence.schema.show.v1";
const SCHEMA_VALIDATION_REPORT: &str = "assay.evidence.schema.validation.v1";

const PROMPTFOO_RECEIPT_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/reference/receipt-schemas/receipts/promptfoo.assertion-component.v1.schema.json"
));
const OPENFEATURE_RECEIPT_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/reference/receipt-schemas/receipts/openfeature.evaluation-details.v1.schema.json"
));
const CYCLONEDX_RECEIPT_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/reference/receipt-schemas/receipts/cyclonedx.mlbom-model-component.v1.schema.json"
));
const MASTRA_RECEIPT_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/reference/receipt-schemas/receipts/mastra.score-event.v1.schema.json"
));
const PROMPTFOO_INPUT_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/reference/receipt-schemas/inputs/promptfoo-cli-jsonl-component-result.v1.schema.json"
));
const OPENFEATURE_INPUT_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/reference/receipt-schemas/inputs/openfeature-evaluation-details-export.v1.schema.json"
));
const CYCLONEDX_INPUT_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/reference/receipt-schemas/inputs/cyclonedx-mlbom-model-component-input.v1.schema.json"
));
const MASTRA_INPUT_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/reference/receipt-schemas/inputs/mastra-score-event-export.v1.schema.json"
));

/// Inspect and validate supported receipt schema contracts.
#[derive(Debug, Args, Clone)]
pub struct SchemaArgs {
    #[command(subcommand)]
    pub cmd: SchemaCmd,
}

#[derive(Debug, Subcommand, Clone)]
pub enum SchemaCmd {
    /// List supported receipt and importer-input schemas
    List(SchemaListArgs),
    /// Show schema metadata, or the raw JSON Schema with --raw
    Show(SchemaShowArgs),
    /// Validate a JSON or JSONL artifact against a supported schema
    Validate(SchemaValidateArgs),
}

#[derive(Debug, Args, Clone)]
pub struct SchemaListArgs {
    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,
}

#[derive(Debug, Args, Clone)]
pub struct SchemaShowArgs {
    /// Schema name, alias, source path, or JSON Schema $id
    #[arg(value_name = "SCHEMA")]
    pub schema: String,

    /// Output format for metadata
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,

    /// Print the raw JSON Schema instead of metadata
    #[arg(long)]
    pub raw: bool,
}

#[derive(Debug, Args, Clone)]
pub struct SchemaValidateArgs {
    /// Schema name, alias, source path, or JSON Schema $id
    #[arg(long)]
    pub schema: String,

    /// JSON or JSONL artifact to validate
    #[arg(long)]
    pub input: PathBuf,

    /// Treat input as JSONL and validate each non-empty row
    #[arg(long)]
    pub jsonl: bool,

    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum SchemaKind {
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

#[derive(Clone, Copy)]
struct SchemaDescriptor {
    name: &'static str,
    aliases: &'static [&'static str],
    kind: SchemaKind,
    status: &'static str,
    family: &'static str,
    source_path: &'static str,
    description: &'static str,
    trust_basis_claim: Option<&'static str>,
    importer_only: bool,
    schema_json: &'static str,
}

impl SchemaDescriptor {
    fn json_schema_value(&self) -> Result<Value> {
        serde_json::from_str(self.schema_json)
            .with_context(|| format!("failed to parse embedded schema {}", self.name))
    }

    fn json_schema_id(&self) -> Result<String> {
        Ok(self
            .json_schema_value()?
            .get("$id")
            .and_then(Value::as_str)
            .unwrap_or(self.name)
            .to_string())
    }

    fn matches(&self, needle: &str) -> Result<bool> {
        if self.name == needle || self.source_path == needle || self.aliases.contains(&needle) {
            return Ok(true);
        }
        Ok(self.json_schema_id()? == needle)
    }
}

const SCHEMAS: &[SchemaDescriptor] = &[
    SchemaDescriptor {
        name: "promptfoo.assertion-component.v1",
        aliases: &["assay.receipt.promptfoo.assertion-component.v1"],
        kind: SchemaKind::Receipt,
        status: "stable",
        family: "external_eval_receipts",
        source_path: "docs/reference/receipt-schemas/receipts/promptfoo.assertion-component.v1.schema.json",
        description: "Assay receipt payload for one selected Promptfoo assertion component result.",
        trust_basis_claim: Some("external_eval_receipt_boundary_visible"),
        importer_only: false,
        schema_json: PROMPTFOO_RECEIPT_SCHEMA,
    },
    SchemaDescriptor {
        name: "openfeature.evaluation-details.v1",
        aliases: &["assay.receipt.openfeature.evaluation_details.v1"],
        kind: SchemaKind::Receipt,
        status: "stable",
        family: "external_decision_receipts",
        source_path: "docs/reference/receipt-schemas/receipts/openfeature.evaluation-details.v1.schema.json",
        description: "Assay receipt payload for one bounded OpenFeature boolean EvaluationDetails decision.",
        trust_basis_claim: Some("external_decision_receipt_boundary_visible"),
        importer_only: false,
        schema_json: OPENFEATURE_RECEIPT_SCHEMA,
    },
    SchemaDescriptor {
        name: "cyclonedx.mlbom-model-component.v1",
        aliases: &["assay.receipt.cyclonedx.mlbom-model-component.v1"],
        kind: SchemaKind::Receipt,
        status: "stable",
        family: "external_inventory_receipts",
        source_path: "docs/reference/receipt-schemas/receipts/cyclonedx.mlbom-model-component.v1.schema.json",
        description: "Assay receipt payload for one selected CycloneDX ML-BOM machine-learning-model component.",
        trust_basis_claim: Some("external_inventory_receipt_boundary_visible"),
        importer_only: false,
        schema_json: CYCLONEDX_RECEIPT_SCHEMA,
    },
    SchemaDescriptor {
        name: "mastra.score-event.v1",
        aliases: &["assay.receipt.mastra.score_event.v1"],
        kind: SchemaKind::Receipt,
        status: "experimental",
        family: "score_receipts",
        source_path: "docs/reference/receipt-schemas/receipts/mastra.score-event.v1.schema.json",
        description: "Assay receipt payload for one bounded Mastra ScoreEvent-derived score artifact.",
        trust_basis_claim: None,
        importer_only: true,
        schema_json: MASTRA_RECEIPT_SCHEMA,
    },
    SchemaDescriptor {
        name: "promptfoo-cli-jsonl-component-result.v1",
        aliases: &[],
        kind: SchemaKind::Input,
        status: "stable",
        family: "external_eval_receipts",
        source_path: "docs/reference/receipt-schemas/inputs/promptfoo-cli-jsonl-component-result.v1.schema.json",
        description: "Supported Promptfoo CLI JSONL row shape containing assertion component results.",
        trust_basis_claim: Some("external_eval_receipt_boundary_visible"),
        importer_only: false,
        schema_json: PROMPTFOO_INPUT_SCHEMA,
    },
    SchemaDescriptor {
        name: "openfeature-evaluation-details-export.v1",
        aliases: &[],
        kind: SchemaKind::Input,
        status: "stable",
        family: "external_decision_receipts",
        source_path: "docs/reference/receipt-schemas/inputs/openfeature-evaluation-details-export.v1.schema.json",
        description: "Supported reduced OpenFeature boolean EvaluationDetails JSONL row shape.",
        trust_basis_claim: Some("external_decision_receipt_boundary_visible"),
        importer_only: false,
        schema_json: OPENFEATURE_INPUT_SCHEMA,
    },
    SchemaDescriptor {
        name: "cyclonedx-mlbom-model-component-input.v1",
        aliases: &[],
        kind: SchemaKind::Input,
        status: "stable",
        family: "external_inventory_receipts",
        source_path: "docs/reference/receipt-schemas/inputs/cyclonedx-mlbom-model-component-input.v1.schema.json",
        description: "Supported CycloneDX ML-BOM input shape for selecting one machine-learning-model component.",
        trust_basis_claim: Some("external_inventory_receipt_boundary_visible"),
        importer_only: false,
        schema_json: CYCLONEDX_INPUT_SCHEMA,
    },
    SchemaDescriptor {
        name: "mastra-score-event-export.v1",
        aliases: &[],
        kind: SchemaKind::Input,
        status: "experimental",
        family: "score_receipts",
        source_path: "docs/reference/receipt-schemas/inputs/mastra-score-event-export.v1.schema.json",
        description: "Supported reduced Mastra ScoreEvent JSONL row shape.",
        trust_basis_claim: None,
        importer_only: true,
        schema_json: MASTRA_INPUT_SCHEMA,
    },
];

#[derive(Debug, Serialize)]
struct SchemaListReport {
    schema: &'static str,
    schemas: Vec<SchemaMetadata>,
}

#[derive(Debug, Serialize)]
struct SchemaShowReport {
    schema: &'static str,
    metadata: SchemaMetadata,
}

#[derive(Debug, Serialize)]
struct SchemaMetadata {
    name: String,
    kind: SchemaKind,
    status: String,
    family: String,
    json_schema_id: String,
    source_path: String,
    description: String,
    trust_basis_claim: Option<String>,
    importer_only: bool,
    aliases: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SchemaValidationReport {
    schema: &'static str,
    schema_name: String,
    schema_kind: SchemaKind,
    input: String,
    jsonl: bool,
    valid: bool,
    documents: usize,
    errors: Vec<SchemaValidationError>,
}

#[derive(Debug, Serialize)]
struct SchemaValidationError {
    document: usize,
    instance_path: String,
    message: String,
}

pub fn cmd_schema(args: SchemaArgs) -> Result<i32> {
    match args.cmd {
        SchemaCmd::List(args) => cmd_list(args),
        SchemaCmd::Show(args) => cmd_show(args),
        SchemaCmd::Validate(args) => cmd_validate(args),
    }
}

fn cmd_list(args: SchemaListArgs) -> Result<i32> {
    let report = SchemaListReport {
        schema: SCHEMA_LIST_REPORT,
        schemas: SCHEMAS
            .iter()
            .map(schema_metadata)
            .collect::<Result<Vec<_>>>()?,
    };

    match args.format {
        OutputFormat::Text => write_list_text(&report),
        OutputFormat::Json => write_json(&report),
    }
}

fn cmd_show(args: SchemaShowArgs) -> Result<i32> {
    let descriptor = find_schema(&args.schema)?;
    if args.raw {
        let raw = descriptor.json_schema_value()?;
        println!("{}", serde_json::to_string_pretty(&raw)?);
        return Ok(EXIT_SUCCESS);
    }

    let report = SchemaShowReport {
        schema: SCHEMA_SHOW_REPORT,
        metadata: schema_metadata(descriptor)?,
    };

    match args.format {
        OutputFormat::Text => write_show_text(&report),
        OutputFormat::Json => write_json(&report),
    }
}

fn cmd_validate(args: SchemaValidateArgs) -> Result<i32> {
    let descriptor = find_schema(&args.schema)?;
    let input = std::fs::read_to_string(&args.input).with_context(|| {
        format!(
            "failed to read schema validation input {}",
            args.input.display()
        )
    })?;
    let schema_value = descriptor.json_schema_value()?;
    let validator = jsonschema::options()
        .with_draft(Draft::Draft202012)
        .build(&schema_value)
        .with_context(|| format!("failed to compile schema {}", descriptor.name))?;
    let report = validate_input(descriptor, &args.input, args.jsonl, &input, &validator);

    match args.format {
        OutputFormat::Text => write_validation_text(&report),
        OutputFormat::Json => write_json(&report),
    }?;

    if report.valid {
        Ok(EXIT_SUCCESS)
    } else {
        Ok(EXIT_TEST_FAILURE)
    }
}

fn find_schema(raw: &str) -> Result<&'static SchemaDescriptor> {
    let needle = raw.trim();
    if needle.is_empty() {
        bail!("schema name is empty");
    }

    for descriptor in SCHEMAS {
        if descriptor.matches(needle)? {
            return Ok(descriptor);
        }
    }

    bail!(
        "unknown receipt schema {needle:?}; run `assay evidence schema list` for supported schemas"
    );
}

fn schema_metadata(descriptor: &SchemaDescriptor) -> Result<SchemaMetadata> {
    Ok(SchemaMetadata {
        name: descriptor.name.to_string(),
        kind: descriptor.kind,
        status: descriptor.status.to_string(),
        family: descriptor.family.to_string(),
        json_schema_id: descriptor.json_schema_id()?,
        source_path: descriptor.source_path.to_string(),
        description: descriptor.description.to_string(),
        trust_basis_claim: descriptor.trust_basis_claim.map(str::to_string),
        importer_only: descriptor.importer_only,
        aliases: descriptor
            .aliases
            .iter()
            .map(|alias| (*alias).to_string())
            .collect(),
    })
}

fn validate_input(
    descriptor: &SchemaDescriptor,
    input_path: &std::path::Path,
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
                instance_path: "$".to_string(),
                message: format!("invalid JSON: {err}"),
            }),
        }
    }

    if documents == 0 {
        errors.push(SchemaValidationError {
            document: 0,
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
                instance_path: err.instance_path().to_string(),
                message: err.to_string(),
            }),
    );
}

fn write_list_text(report: &SchemaListReport) -> Result<i32> {
    println!("Assay Receipt Schema Registry");
    println!("Schema: {}", report.schema);
    for schema in &report.schemas {
        println!(
            "- {} [{}; {}; family: {}]",
            schema.name, schema.kind, schema.status, schema.family
        );
        println!("  id: {}", schema.json_schema_id);
        println!("  source: {}", schema.source_path);
        println!("  description: {}", schema.description);
        if let Some(claim) = &schema.trust_basis_claim {
            println!("  trust_basis_claim: {claim}");
        } else if schema.importer_only {
            println!("  trust_basis_claim: none (importer-only)");
        }
    }
    Ok(EXIT_SUCCESS)
}

fn write_show_text(report: &SchemaShowReport) -> Result<i32> {
    let schema = &report.metadata;
    println!("Schema: {}", schema.name);
    println!("Kind: {}", schema.kind);
    println!("Status: {}", schema.status);
    println!("Family: {}", schema.family);
    println!("JSON Schema $id: {}", schema.json_schema_id);
    println!("Source: {}", schema.source_path);
    println!("Description: {}", schema.description);
    if let Some(claim) = &schema.trust_basis_claim {
        println!("Trust Basis claim: {claim}");
    } else if schema.importer_only {
        println!("Trust Basis claim: none (importer-only)");
    } else {
        println!("Trust Basis claim: none");
    }
    if !schema.aliases.is_empty() {
        println!("Aliases:");
        for alias in &schema.aliases {
            println!("- {alias}");
        }
    }
    Ok(EXIT_SUCCESS)
}

fn write_validation_text(report: &SchemaValidationReport) -> Result<i32> {
    if report.valid {
        println!(
            "Schema validation passed: {} matches {} ({} document(s))",
            report.input, report.schema_name, report.documents
        );
        return Ok(EXIT_SUCCESS);
    }

    println!(
        "Schema validation failed: {} does not match {}",
        report.input, report.schema_name
    );
    for error in &report.errors {
        println!(
            "- document {} at {}: {}",
            error.document, error.instance_path, error.message
        );
    }
    Ok(EXIT_SUCCESS)
}

fn write_json<T: Serialize>(value: &T) -> Result<i32> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(EXIT_SUCCESS)
}
