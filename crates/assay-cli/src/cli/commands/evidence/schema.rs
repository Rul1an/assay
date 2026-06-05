mod registry;
mod reports;
mod validate;
mod write;

use crate::cli::args::OutputFormat;
use crate::exit_codes::{EXIT_CONFIG_ERROR, EXIT_SUCCESS, EXIT_TEST_FAILURE};
use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use jsonschema::Draft;
use std::path::PathBuf;

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

pub fn cmd_schema(args: SchemaArgs) -> Result<i32> {
    match args.cmd {
        SchemaCmd::List(args) => cmd_list(args),
        SchemaCmd::Show(args) => cmd_show(args),
        SchemaCmd::Validate(args) => cmd_validate(args),
    }
}

fn cmd_list(args: SchemaListArgs) -> Result<i32> {
    let report = reports::SchemaListReport {
        schema: reports::SCHEMA_LIST_REPORT,
        schemas: registry::SCHEMAS
            .iter()
            .map(registry::schema_metadata)
            .collect::<Result<Vec<_>>>()?,
    };

    match args.format {
        OutputFormat::Text => write::write_list_text(&report),
        OutputFormat::Json => write::write_json(&report),
    }
}

fn cmd_show(args: SchemaShowArgs) -> Result<i32> {
    let descriptor = registry::find_schema(&args.schema)?;
    if args.raw {
        let raw = descriptor.json_schema_value()?;
        return write::write_json(&raw);
    }

    let report = reports::SchemaShowReport {
        schema: reports::SCHEMA_SHOW_REPORT,
        metadata: registry::schema_metadata(descriptor)?,
    };

    match args.format {
        OutputFormat::Text => write::write_show_text(&report),
        OutputFormat::Json => write::write_json(&report),
    }
}

fn cmd_validate(args: SchemaValidateArgs) -> Result<i32> {
    let descriptor = registry::find_schema(&args.schema)?;
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
    let report = validate::validate_input(descriptor, &args.input, args.jsonl, &input, &validator);

    match args.format {
        OutputFormat::Text => write::write_validation_text(&report),
        OutputFormat::Json => write::write_json(&report),
    }?;

    if report.valid {
        Ok(EXIT_SUCCESS)
    } else if report.has_input_errors() {
        Ok(EXIT_CONFIG_ERROR)
    } else {
        Ok(EXIT_TEST_FAILURE)
    }
}
