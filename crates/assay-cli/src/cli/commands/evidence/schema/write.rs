use super::reports::{SchemaListReport, SchemaShowReport, SchemaValidationReport};
use crate::exit_codes::EXIT_SUCCESS;
use anyhow::Result;
use serde::Serialize;

pub(super) fn write_list_text(report: &SchemaListReport) -> Result<i32> {
    println!("Assay Evidence Schema Registry");
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

pub(super) fn write_show_text(report: &SchemaShowReport) -> Result<i32> {
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

pub(super) fn write_validation_text(report: &SchemaValidationReport) -> Result<i32> {
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

pub(super) fn write_json<T: Serialize>(value: &T) -> Result<i32> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(EXIT_SUCCESS)
}
