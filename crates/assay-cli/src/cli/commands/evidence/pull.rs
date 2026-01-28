//! `assay evidence pull` - Download an evidence bundle from storage.

use anyhow::{Context, Result};
use assay_evidence::store::BundleStore;
use assay_evidence::{ObjectStoreBundleStore, StoreError, StoreSpec};
use clap::Args;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Args, Clone)]
pub struct PullArgs {
    /// Bundle ID to download (e.g., sha256:abc123...)
    #[arg(long, required_unless_present = "run_id")]
    pub bundle_id: Option<String>,

    /// Download all bundles for a run ID
    #[arg(long)]
    pub run_id: Option<String>,

    /// Output path (file for single bundle, directory for run)
    #[arg(long, short = 'o', default_value = ".")]
    pub out: PathBuf,

    /// Store URL (e.g., s3://bucket/prefix, file:///path)
    #[arg(long, env = "ASSAY_STORE_URL")]
    pub store: String,

    /// Verify bundle after download
    #[arg(long)]
    pub verify: bool,
}

pub async fn cmd_pull(args: PullArgs) -> Result<i32> {
    // Connect to store
    let spec = StoreSpec::parse(&args.store)
        .with_context(|| format!("invalid store URL: {}", args.store))?;

    let store = ObjectStoreBundleStore::from_spec(&spec)
        .await
        .with_context(|| "failed to connect to store")?;

    if let Some(bundle_id) = &args.bundle_id {
        // Single bundle download
        pull_single(&store, bundle_id, &args.out, args.verify).await
    } else if let Some(run_id) = &args.run_id {
        // Download all bundles for a run
        pull_run(&store, run_id, &args.out, args.verify).await
    } else {
        anyhow::bail!("Either --bundle-id or --run-id is required");
    }
}

async fn pull_single(
    store: &ObjectStoreBundleStore,
    bundle_id: &str,
    out: &Path,
    verify: bool,
) -> Result<i32> {
    eprintln!("Downloading: {}", bundle_id);

    let bytes = match store.get_bundle(bundle_id).await {
        Ok(b) => b,
        Err(StoreError::NotFound { .. }) => {
            eprintln!("❌ Bundle not found: {}", bundle_id);
            return Ok(2); // Exit code 2 for not found
        }
        Err(e) => return Err(e).context("failed to download bundle"),
    };

    // Determine output path
    let out_path = if out.is_dir() {
        let filename = format!("{}.tar.gz", bundle_id.replace(':', "_"));
        out.join(filename)
    } else {
        out.to_path_buf()
    };

    // Write to file
    let mut file = File::create(&out_path)
        .with_context(|| format!("failed to create output file: {}", out_path.display()))?;

    file.write_all(&bytes)
        .with_context(|| "failed to write bundle")?;

    eprintln!("✅ Downloaded to: {}", out_path.display());

    // Verify if requested
    if verify {
        let cursor = std::io::Cursor::new(bytes.as_ref());
        assay_evidence::verify_bundle(cursor).context("bundle verification failed")?;
        eprintln!("✅ Verified: OK");
    }

    Ok(0)
}

async fn pull_run(
    store: &ObjectStoreBundleStore,
    run_id: &str,
    out_dir: &PathBuf,
    verify: bool,
) -> Result<i32> {
    // Ensure output is a directory
    if out_dir.exists() && !out_dir.is_dir() {
        anyhow::bail!("Output path must be a directory when using --run-id");
    }

    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("failed to create output directory: {}", out_dir.display()))?;

    // List bundles for run
    let bundle_ids = store
        .list_bundles_for_run(run_id)
        .await
        .with_context(|| format!("failed to list bundles for run: {}", run_id))?;

    if bundle_ids.is_empty() {
        eprintln!("⚠️  No bundles found for run: {}", run_id);
        return Ok(0);
    }

    eprintln!("Found {} bundle(s) for run: {}", bundle_ids.len(), run_id);

    let mut errors = 0;
    for bundle_id in &bundle_ids {
        match pull_single(store, bundle_id, out_dir, verify).await {
            Ok(0) => {}
            Ok(code) => {
                errors += 1;
                eprintln!("Warning: bundle {} returned exit code {}", bundle_id, code);
            }
            Err(e) => {
                errors += 1;
                eprintln!("Error downloading {}: {}", bundle_id, e);
            }
        }
    }

    if errors > 0 {
        eprintln!(
            "⚠️  Completed with {} error(s) out of {} bundle(s)",
            errors,
            bundle_ids.len()
        );
        Ok(1)
    } else {
        eprintln!("✅ Downloaded {} bundle(s)", bundle_ids.len());
        Ok(0)
    }
}
