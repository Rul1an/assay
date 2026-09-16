//! `assay evidence push` - Upload an evidence bundle to storage.

use anyhow::{Context, Result};
use assay_common::limits::{LimitKind, LimitReader};
use assay_evidence::bundle::writer::VerifyLimits;
use assay_evidence::store::{BundleStore, StreamCeiling};
use assay_evidence::{resolve_store_url, Bytes, ObjectStoreBundleStore, StoreError, StoreSpec};
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

    /// Refused. The store key is the verified `bundle_id`, so an unverified archive has no key.
    #[arg(long)]
    pub no_verify: bool,

    /// Continue even if bundle already exists
    #[arg(long)]
    pub allow_exists: bool,
}

pub async fn cmd_push(args: PushArgs) -> Result<i32> {
    // The object key is `bundle_id`, and the only `bundle_id` that means anything is the one the
    // verifier has pinned to the recomputed `run_root` (check 14). `--no-verify` used to read the
    // id out of the unverified manifest and upload under it, so an archive named its own key and
    // "content-addressed" held only on the verifying branch. There is no key to derive from an
    // unverified archive, so the flag is refused before the file is opened: nothing is read,
    // nothing is uploaded, no run is linked. Exit 2 is the usage-error code (#2492 slice 1).
    if args.no_verify {
        eprintln!(
            "❌ --no-verify is not accepted by `evidence push`: the store key is the verified \
             bundle_id, and an unverified archive cannot choose its own key."
        );
        eprintln!("   Nothing was read or uploaded. Run without --no-verify.");
        return Ok(2);
    }

    // 1. Read bundle
    let mut file = File::open(&args.bundle)
        .with_context(|| format!("failed to open bundle: {}", args.bundle.display()))?;

    // ADR-043 section 1: the ceiling applies to the stream, before the input is materialized.
    // The file was read whole with no bound at all, so an oversized archive sized the allocation
    // regardless of what the verifier concluded afterwards. Whatever is about to be uploaded has
    // to pass the ceiling first.
    let limits = VerifyLimits::default();
    let mut buffer = Vec::new();
    LimitReader::new(&mut file, limits.max_bundle_bytes, LimitKind::SourceBytes)
        .read_to_end(&mut buffer)
        .with_context(|| "failed to read bundle")?;

    // 2. Verify bundle; the key is the verified id and nothing else.
    let cursor = std::io::Cursor::new(&buffer);
    let result = assay_evidence::bundle::writer::verify_bundle_with_limits(cursor, limits)
        .context("bundle verification failed")?;
    eprintln!("✅ Bundle verified: {}", result.manifest.bundle_id);
    let bundle_id = result.manifest.bundle_id;

    // The upload takes ownership of the same buffer that was just checked rather than cloning it.
    // The two copies that remain are the source snapshot and the upload bytes.
    let bytes = Bytes::from(buffer);

    // 3. Connect to store
    let url = resolve_store_url(args.store.as_deref(), args.store_config.as_deref())
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let spec = StoreSpec::parse(&url).with_context(|| format!("invalid store URL: {}", url))?;

    let store = ObjectStoreBundleStore::from_spec(&spec)
        .await
        .with_context(|| "failed to connect to store")?;

    // 4. Upload bundle
    match store.put_bundle(&bundle_id, bytes.clone()).await {
        Ok(()) => {
            eprintln!("✅ Uploaded: {}", bundle_id);
        }
        Err(StoreError::AlreadyExists { .. }) => {
            // The key is `bundle_id`, which is the bundle's `run_root`: a digest over event
            // semantics, not over the archive. A different archive with the same events shares it.
            // So an existing object is an idempotent re-upload only if it is these bytes; anything
            // else would keep the other archive and, below, link this run to it. `--allow-exists`
            // quiets the identical case and does not waive this.
            let stored = store
                .get_bundle_bounded(&bundle_id, StreamCeiling::new(limits.max_bundle_bytes))
                .await
                .with_context(|| {
                    format!(
                        "bundle {bundle_id} already exists and could not be read back to compare"
                    )
                })?;
            if stored != bytes {
                anyhow::bail!(
                    "a different archive is already stored as {bundle_id}; it was not replaced, \
                     and no run was linked"
                );
            }
            if args.allow_exists {
                eprintln!("ℹ️  Bundle already exists: {}", bundle_id);
            } else {
                eprintln!("⚠️  Bundle already exists: {}", bundle_id);
                eprintln!("   Use --allow-exists to suppress this warning");
                // Not an error: the stored object is these exact bytes.
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
