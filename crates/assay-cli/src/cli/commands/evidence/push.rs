//! `assay evidence push` - Upload an evidence bundle to storage.

use anyhow::{Context, Result};
use assay_evidence::store::BundleStore;
use assay_evidence::{
    resolve_store_url, verify_bundle, Bytes, ObjectStoreBundleStore, StoreError, StoreSpec,
};
use clap::Args;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

#[derive(Debug, Args, Clone)]
pub struct PushArgs {
    /// Path to the evidence bundle (.tar.gz)
    #[arg(value_name = "BUNDLE")]
    pub bundle: PathBuf,

    /// Run ID to link this bundle to (for `list --run-id`)
    #[arg(long)]
    pub run_id: Option<String>,

    /// Store URL (e.g., s3://bucket/prefix, file:///path)
    #[arg(long, env = "ASSAY_STORE_URL")]
    pub store: Option<String>,

    /// Path to store config YAML (default: .assay/store.yaml)
    #[arg(long)]
    pub store_config: Option<PathBuf>,

    /// Skip verification before upload
    #[arg(long)]
    pub no_verify: bool,

    /// Continue even if bundle already exists
    #[arg(long)]
    pub allow_exists: bool,
}

pub async fn cmd_push(args: PushArgs) -> Result<i32> {
    // 1. Read bundle
    let mut file = File::open(&args.bundle)
        .with_context(|| format!("failed to open bundle: {}", args.bundle.display()))?;

    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)
        .with_context(|| "failed to read bundle")?;

    let bytes = Bytes::from(buffer.clone());

    // 2. Verify bundle (unless --no-verify)
    let bundle_id = if args.no_verify {
        let cursor = std::io::Cursor::new(&buffer);
        let reader = assay_evidence::BundleReader::open_unverified(cursor)
            .context("failed to read bundle manifest")?;
        reader.manifest().bundle_id.clone()
    } else {
        let cursor = std::io::Cursor::new(&buffer);
        let result = verify_bundle(cursor).context("bundle verification failed")?;
        eprintln!("✅ Bundle verified: {}", result.manifest.bundle_id);
        result.manifest.bundle_id
    };

    // 3. Connect to store
    let url = resolve_store_url(args.store.as_deref(), args.store_config.as_deref())
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let spec = StoreSpec::parse(&url).with_context(|| format!("invalid store URL: {}", url))?;

    let store = ObjectStoreBundleStore::from_spec(&spec)
        .await
        .with_context(|| "failed to connect to store")?;

    // 4. Upload bundle
    match store.put_bundle(&bundle_id, bytes).await {
        Ok(()) => {
            eprintln!("✅ Uploaded: {}", bundle_id);
        }
        Err(StoreError::AlreadyExists { .. }) => {
            if args.allow_exists {
                eprintln!("ℹ️  Bundle already exists: {}", bundle_id);
            } else {
                eprintln!("⚠️  Bundle already exists: {}", bundle_id);
                eprintln!("   Use --allow-exists to suppress this warning");
                // Not an error - idempotent
            }
        }
        Err(e) => {
            return Err(e).context("failed to upload bundle");
        }
    }

    // 5. Link to run_id if provided
    if let Some(run_id) = &args.run_id {
        store
            .link_run_bundle(run_id, &bundle_id)
            .await
            .with_context(|| format!("failed to link bundle to run {}", run_id))?;
        eprintln!("✅ Linked to run: {}", run_id);
    }

    Ok(0)
}
