//! `assay evidence index rebuild` - Rebuild the run-to-bundle index from canonical bundles.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Subcommand, ValueEnum};

use assay_evidence::store::{IndexRebuildReport, StreamCeiling};
use assay_evidence::{resolve_store_url, ObjectStoreBundleStore, StoreSpec, VerifyLimits};

pub const SCHEMA_INDEX_REBUILD_V0: &str = "assay.evidence.index_rebuild.v0";

#[derive(Debug, Subcommand, Clone)]
pub enum EvidenceIndexCmd {
    /// Rebuild the run-to-bundle index from canonical bundles
    Rebuild(IndexRebuildArgs),
}

#[derive(Debug, Args, Clone)]
pub struct IndexRebuildArgs {
    /// Store URL (e.g., s3://bucket/prefix, file:///path)
    #[arg(long, env = "ASSAY_STORE_URL")]
    pub store: Option<String>,

    /// Path to store config YAML (default: .assay/store.yaml)
    #[arg(long)]
    pub store_config: Option<PathBuf>,

    /// Output format
    #[arg(long, value_enum, default_value = "summary")]
    pub format: IndexRebuildFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum IndexRebuildFormat {
    #[default]
    Summary,
    Json,
}

pub async fn cmd_evidence_index(cmd: EvidenceIndexCmd) -> Result<i32> {
    match cmd {
        EvidenceIndexCmd::Rebuild(args) => cmd_index_rebuild(args).await,
    }
}

pub async fn cmd_index_rebuild(args: IndexRebuildArgs) -> Result<i32> {
    let url = match resolve_store_url(args.store.as_deref(), args.store_config.as_deref()) {
        Ok(u) => u,
        Err(e) => {
            eprintln!("Config error: {}", e);
            return Ok(2);
        }
    };

    let spec = match StoreSpec::parse(&url) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Config error: invalid store URL '{}': {}", url, e);
            return Ok(2);
        }
    };

    let store = match ObjectStoreBundleStore::from_spec(&spec).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Connection error: {}", e);
            return Ok(1);
        }
    };

    let ceiling = StreamCeiling::new(VerifyLimits::default().max_bundle_bytes);
    let mut report = match store.rebuild_index(ceiling).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Rebuild error: {}", e);
            return Ok(1);
        }
    };
    report.schema = SCHEMA_INDEX_REBUILD_V0.to_string();

    match args.format {
        IndexRebuildFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        IndexRebuildFormat::Summary => {
            print_human_summary(&report, &url);
        }
    }

    if report.failed_bundles.is_empty() {
        Ok(0)
    } else {
        Ok(1)
    }
}

fn print_human_summary(report: &IndexRebuildReport, url: &str) {
    println!("Evidence Index Rebuild");
    println!("======================");
    println!();
    println!("  Store:             {}", url);
    println!(
        "  Discovered:        {} bundle(s)",
        report.discovered_bundles
    );
    println!("  Verified:          {}", report.verified_bundles);
    println!("  Failed:            {}", report.failed_bundles.len());
    println!("  Refs linked:       {}", report.refs_linked);
    println!("  Already indexed:   {}", report.refs_already_indexed);
    println!("  Stale refs:        {}", report.stale_refs.len());

    if !report.stale_refs.is_empty() {
        println!();
        println!("  ⚠️  Stale references detected (not deleted):");
        for s in &report.stale_refs {
            println!(
                "     - run: {}, bundle: {} (missing from store)",
                s.run_id, s.bundle_id
            );
        }
    }

    if !report.failed_bundles.is_empty() {
        println!();
        println!("  ❌ Failed bundles (skipped):");
        for f in &report.failed_bundles {
            println!("     - {}: {}", f.bundle_id, f.reason);
        }
    }
}
