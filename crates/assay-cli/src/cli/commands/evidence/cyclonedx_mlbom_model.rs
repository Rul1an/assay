mod constants;
mod events;
mod reduce;
mod source;
#[cfg(test)]
mod tests;
mod validate;

use crate::cli::commands::evidence::cyclonedx_mlbom_model::constants::DEFAULT_RUN_ID;
use crate::cli::commands::evidence::cyclonedx_mlbom_model::events::read_cyclonedx_model_event;
use crate::cli::commands::evidence::cyclonedx_mlbom_model::source::{
    default_source_artifact_ref, parse_import_time, sha256_file,
};
use crate::exit_codes;
use anyhow::{Context, Result};
use assay_evidence::bundle::BundleWriter;
use assay_evidence::types::ProducerMeta;
use clap::Args;
use std::fs::File;
use std::path::PathBuf;

#[derive(Debug, Args, Clone)]
pub struct CycloneDxMlBomModelArgs {
    /// CycloneDX JSON BOM artifact file
    #[arg(long, value_name = "PATH")]
    pub input: PathBuf,

    /// Output Assay evidence bundle path (.tar.gz)
    #[arg(long, alias = "out", value_name = "PATH")]
    pub bundle_out: PathBuf,

    /// Select a machine-learning-model component by bom-ref
    #[arg(long)]
    pub bom_ref: Option<String>,

    /// Reviewer-safe source artifact reference stored in receipts
    #[arg(long)]
    pub source_artifact_ref: Option<String>,

    /// Assay import run id used for receipt provenance and event ids
    #[arg(long, default_value = DEFAULT_RUN_ID)]
    pub run_id: String,

    /// Import timestamp for deterministic fixtures (RFC3339 UTC recommended)
    #[arg(long)]
    pub import_time: Option<String>,
}

pub fn cmd_cyclonedx_mlbom_model(args: CycloneDxMlBomModelArgs) -> Result<i32> {
    let import_time = parse_import_time(args.import_time.as_deref())?;
    let source_artifact_ref = args
        .source_artifact_ref
        .unwrap_or_else(|| default_source_artifact_ref(&args.input));
    // The receipt reduces one selected model component, while provenance binds
    // back to the exact BOM bytes that carried the richer inventory.
    let source_artifact_digest = sha256_file(&args.input)
        .with_context(|| format!("failed to digest input {}", args.input.display()))?;
    let producer = ProducerMeta {
        name: "assay-cli".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        git: option_env!("ASSAY_GIT_SHA").map(str::to_string),
    };

    let event = read_cyclonedx_model_event(
        &args.input,
        args.bom_ref.as_deref(),
        &source_artifact_ref,
        &source_artifact_digest,
        &args.run_id,
        import_time,
        &producer,
    )?;

    let out_file = File::create(&args.bundle_out)
        .with_context(|| format!("failed to create bundle {}", args.bundle_out.display()))?;
    let mut writer = BundleWriter::new(out_file).with_producer(producer);
    writer.add_event(event);
    writer
        .finish()
        .with_context(|| format!("failed to write bundle {}", args.bundle_out.display()))?;

    eprintln!(
        "Imported CycloneDX ML-BOM model component receipt to {}",
        args.bundle_out.display()
    );

    Ok(exit_codes::OK)
}
