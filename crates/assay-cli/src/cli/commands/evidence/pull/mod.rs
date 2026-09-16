//! `assay evidence pull` - Download an evidence bundle from storage.

use anyhow::{Context, Result};
use assay_evidence::sanitize::sanitize_terminal;
use assay_evidence::store::BundleStore;
use assay_evidence::store::{BoundedGetError, StreamCeiling};
use assay_evidence::{
    resolve_store_url, ErrorClass, ErrorCode, ObjectStoreBundleStore, StoreError, StoreSpec,
    VerifyError, VerifyLimits,
};
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
    pub store: Option<String>,

    /// Path to store config YAML (default: .assay/store.yaml)
    #[arg(long)]
    pub store_config: Option<PathBuf>,

    /// Verify bundle after download
    #[arg(long)]
    pub verify: bool,
}

pub async fn cmd_pull(args: PullArgs) -> Result<i32> {
    let url = resolve_store_url(args.store.as_deref(), args.store_config.as_deref())
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let spec = StoreSpec::parse(&url).with_context(|| format!("invalid store URL: {}", url))?;

    let store = ObjectStoreBundleStore::from_spec(&spec)
        .await
        .with_context(|| "failed to connect to store")?;

    // ADR-043 section 1: the ceiling applies to the source before the input is materialized.
    // `evidence push` bounds its side with `VerifyLimits::default().max_bundle_bytes`; pull takes
    // the same number from the same place, so the two ends of the same transfer agree and there is
    // no second, unrelated default to keep in step. Downloading more than the verifier would ever
    // accept cannot help.
    let ceiling = StreamCeiling::new(VerifyLimits::default().max_bundle_bytes);

    if let Some(bundle_id) = &args.bundle_id {
        // Single bundle download
        pull_single(&store, bundle_id, &args.out, args.verify, ceiling).await
    } else if let Some(run_id) = &args.run_id {
        // Download all bundles for a run
        pull_run(&store, run_id, &args.out, args.verify, ceiling).await
    } else {
        anyhow::bail!("Either --bundle-id or --run-id is required");
    }
}

/// Render a caller- or store-supplied string for a terminal.
///
/// A bundle id and a run id arrive from the command line or from a listing the store produced,
/// and both are echoed straight into a terminal. Escape sequences in either can rewrite the line,
/// hide what was printed, or drive a terminal's own OSC handlers, so nothing untrusted is written
/// raw. Display only: the store lookup always uses the original string, because sanitizing an
/// identifier before a lookup would silently ask for a different object.
fn shown(value: &str) -> String {
    sanitize_terminal(value)
}

/// Build the output filename for a bundle, as a single path component that cannot escape.
///
/// The previous form was `bundle_id.replace(':', "_")` with `.tar.gz` appended, which strips only
/// the character that happens to appear in a well-formed id. A `--bundle-id` of `../../etc/x`
/// survived intact and `out.join` then resolved it outside the directory the operator chose; on
/// Windows a backslash did the same. The id is caller input, and the fix is not to enumerate the
/// dangerous characters but to permit the harmless ones.
///
/// Strict ASCII allowlist, everything else mapped to `_`, so the result is always exactly one
/// component with no separator and no traversal segment. The stored id is untouched — this
/// changes only what the file on disk is called.
fn bundle_filename(bundle_id: &str) -> String {
    let mut stem: String = bundle_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();

    // An id of only rejected characters still yields underscores, so this catches the empty input
    // alone. `.tar.gz` by itself would be a hidden file with no name.
    if stem.is_empty() {
        stem.push_str("bundle");
    }

    format!("{stem}.tar.gz")
}

async fn pull_single(
    store: &ObjectStoreBundleStore,
    bundle_id: &str,
    out: &Path,
    verify: bool,
    ceiling: StreamCeiling,
) -> Result<i32> {
    eprintln!("Downloading: {}", shown(bundle_id));

    // The bounded method rather than the trait's `get_bundle`, which ends in
    // `GetResult::bytes().await` and hands back an object that is already fully in memory. The
    // trait method stays as compatibility surface for other callers; this is the entrypoint the
    // CLI uses, and it is the one ADR-043 section 1 names.
    let bytes = match store.get_bundle_bounded(bundle_id, ceiling).await {
        Ok(b) => b,
        // Every resource refusal in one arm, keyed on the predicate rather than on the variant
        // list. Enumerating variants here would send any dimension added later to the catch-all
        // below, turning a budget refusal into an unrelated exit code — and `BoundedGetError` is
        // `#[non_exhaustive]` precisely so dimensions can be added.
        Err(e) if e.is_resource_refusal() => {
            // Refuse before any output path is computed or created, so a refusal leaves nothing
            // behind for a later run to mistake for a complete download.
            //
            // This says a configured budget was exceeded and nothing else. It is not a claim that
            // the remote bundle is invalid: the download stopped at the chunk that crossed a
            // ceiling, so the bundle was never read far enough to be assessed.
            eprintln!("❌ Download refused: {}", e);
            eprintln!("   The bundle was not downloaded and was not assessed.");
            return Ok(2);
        }
        Err(BoundedGetError::Store(StoreError::NotFound { .. })) => {
            eprintln!("❌ Bundle not found: {}", shown(bundle_id));
            return Ok(2); // Exit code 2 for not found
        }
        Err(e) => return Err(e).context("failed to download bundle"),
    };

    // With --verify, the bytes are verified and bound to the requested id before anything is
    // written. Integrity alone says the store returned *a* valid bundle; the store is keyed by a
    // name anyone with write access can put any object under, so the verified manifest must also
    // name the bundle that was asked for. Writing first, as this used to, left an unverified file
    // under the requested name even when verification then failed.
    if verify {
        let verified = assay_evidence::verify_bundle(std::io::Cursor::new(bytes.as_ref()))
            .context("bundle verification failed")?;
        if verified.manifest.bundle_id != bundle_id {
            // Verifier check 14 pins `bundle_id == run_root` inside the archive under this same
            // code; the key is the one identity it cannot see, so the CLI closes that link with
            // the verifier's own class and code rather than a prose-only error. A typed refusal
            // is what the reason-code registry classifies; prose is indistinguishable from a
            // network failure downstream. The message carries both ids (#2492 slice 1).
            return Err(VerifyError::new(
                ErrorClass::Contract,
                ErrorCode::ContractBundleIdMismatch,
                format!(
                    "requested {}, but the store returned a bundle that identifies as {}; \
                     nothing was written",
                    shown(bundle_id),
                    shown(&verified.manifest.bundle_id)
                ),
            )
            .into());
        }
    }

    // Determine output path
    let out_path = if out.is_dir() {
        out.join(bundle_filename(bundle_id))
    } else {
        out.to_path_buf()
    };

    // Write to file
    let mut file = File::create(&out_path)
        .with_context(|| format!("failed to create output file: {}", out_path.display()))?;

    file.write_all(&bytes)
        .with_context(|| "failed to write bundle")?;

    eprintln!("✅ Downloaded to: {}", out_path.display());

    if verify {
        eprintln!("✅ Verified: OK");
    }

    Ok(0)
}

async fn pull_run(
    store: &ObjectStoreBundleStore,
    run_id: &str,
    out_dir: &PathBuf,
    verify: bool,
    ceiling: StreamCeiling,
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
        .with_context(|| format!("failed to list bundles for run: {}", shown(run_id)))?;

    if bundle_ids.is_empty() {
        eprintln!("⚠️  No bundles found for run: {}", shown(run_id));
        return Ok(0);
    }

    eprintln!(
        "Found {} bundle(s) for run: {}",
        bundle_ids.len(),
        shown(run_id)
    );

    let mut errors = 0;
    for bundle_id in &bundle_ids {
        match pull_single(store, bundle_id, out_dir, verify, ceiling).await {
            Ok(0) => {}
            Ok(code) => {
                errors += 1;
                eprintln!(
                    "Warning: bundle {} returned exit code {}",
                    shown(bundle_id),
                    code
                );
            }
            Err(e) => {
                errors += 1;
                eprintln!("Error downloading {}: {}", shown(bundle_id), e);
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

#[cfg(test)]
mod bounded_pull;
#[cfg(test)]
mod untrusted_identifiers;
#[cfg(test)]
mod verified_pull;
