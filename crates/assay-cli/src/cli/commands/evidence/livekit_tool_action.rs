use crate::exit_codes;
use anyhow::{Context, Result};
use assay_evidence::bundle::BundleWriter;
use assay_evidence::types::ProducerMeta;
use clap::Args;
use std::fs::File;
use std::path::PathBuf;

mod bundle;
mod canonical;
mod constants;
mod input;
mod reduce;
#[cfg(test)]
mod tests;
mod validate;

use constants::DEFAULT_RUN_ID;

#[derive(Debug, Args, Clone)]
pub struct LiveKitToolActionArgs {
    /// LiveKit FunctionToolsExecutedEvent reduced artifact file
    #[arg(long, value_name = "PATH")]
    pub input: PathBuf,

    /// Output Assay evidence bundle path (.tar.gz)
    #[arg(long, alias = "out", value_name = "PATH")]
    pub bundle_out: PathBuf,

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

pub fn cmd_livekit_tool_action(args: LiveKitToolActionArgs) -> Result<i32> {
    let import_time = input::parse_import_time(args.import_time.as_deref())?;
    let source_artifact_ref = args
        .source_artifact_ref
        .unwrap_or_else(|| input::default_source_artifact_ref(&args.input));
    let source_artifact_digest = input::sha256_file(&args.input)
        .with_context(|| format!("failed to digest input {}", args.input.display()))?;
    let producer = ProducerMeta {
        name: "assay-cli".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        git: option_env!("ASSAY_GIT_SHA").map(str::to_string),
    };

    let events = bundle::read_livekit_tool_actions(
        &args.input,
        &source_artifact_ref,
        &source_artifact_digest,
        &args.run_id,
        import_time,
        &producer,
    )?;

    let out_file = File::create(&args.bundle_out)
        .with_context(|| format!("failed to create bundle {}", args.bundle_out.display()))?;
    let mut writer = BundleWriter::new(out_file).with_producer(producer);
    for event in events {
        writer.add_event(event);
    }
    writer
        .finish()
        .with_context(|| format!("failed to write bundle {}", args.bundle_out.display()))?;

    eprintln!(
        "Imported LiveKit tool-action receipts to {}",
        args.bundle_out.display()
    );

    Ok(exit_codes::OK)
}
