//! `assay evidence pull` - Download an evidence bundle from storage.

use anyhow::{Context, Result};
use assay_evidence::sanitize::sanitize_terminal;
use assay_evidence::store::BundleStore;
use assay_evidence::store::{BoundedGetError, StreamCeiling};
use assay_evidence::{
    resolve_store_url, ObjectStoreBundleStore, StoreError, StoreSpec, VerifyLimits,
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
mod bounded_pull {
    use super::*;
    use assay_evidence::store::BundleStore;
    use assay_evidence::Bytes;

    /// An in-memory store holding one bundle of `size` bytes.
    async fn seeded_store(bundle_id: &str, size: usize) -> ObjectStoreBundleStore {
        let store = ObjectStoreBundleStore::memory();
        store
            .put_bundle(bundle_id, Bytes::from(vec![b'x'; size]))
            .await
            .expect("seed");
        store
    }

    fn entries(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .expect("read dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// The property a user can check without reading any code: a refused download leaves no file.
    ///
    /// Asserting only the exit code would not be enough. The old path materialized the object
    /// before the CLI saw it, so "the command failed" and "nothing was written" were separate
    /// facts, and a future change that streams straight to disk would keep the exit code while
    /// leaving a truncated archive behind. A leftover prefix is worse than no file at all: it has
    /// a plausible name and a later run cannot tell it from a complete download.
    #[tokio::test]
    async fn an_oversized_single_pull_writes_no_file() {
        let id = "sha256:toobig";
        let store = seeded_store(id, 10_000).await;
        let out = tempfile::tempdir().expect("tmp");

        let code = pull_single(&store, id, out.path(), false, StreamCeiling::new(100))
            .await
            .expect("a ceiling refusal is an exit code, not an error");

        assert_eq!(code, 2, "a refused download exits 2");
        assert!(
            entries(out.path()).is_empty(),
            "a refused download must leave the output directory empty, found {:?}",
            entries(out.path())
        );
    }

    /// The same property through the run path, which delegates to `pull_single`. Bounding one
    /// function covers both entrypoints, and this is what proves it rather than assuming it.
    #[tokio::test]
    async fn an_oversized_run_pull_writes_no_file() {
        let id = "sha256:toobig";
        let run = "run-42";
        let store = seeded_store(id, 10_000).await;
        store.link_run_bundle(run, id).await.expect("link");
        let out = tempfile::tempdir().expect("tmp");
        let out_dir = out.path().to_path_buf();

        let code = pull_run(&store, run, &out_dir, false, StreamCeiling::new(100))
            .await
            .expect("run pull returns a code");

        assert_eq!(code, 1, "a run with a refused bundle reports errors");
        assert!(
            entries(out.path()).is_empty(),
            "no bundle in the run may be materialized, found {:?}",
            entries(out.path())
        );
    }

    /// The acceptance twin. Same fixture shape, ceiling that fits: the file appears with the exact
    /// bytes. A guard that refused everything would pass both tests above and fail this one.
    #[tokio::test]
    async fn a_bundle_within_the_ceiling_is_written_intact() {
        let id = "sha256:fits";
        let store = seeded_store(id, 4096).await;
        let out = tempfile::tempdir().expect("tmp");

        let code = pull_single(
            &store,
            id,
            out.path(),
            false,
            StreamCeiling::new(100 * 1024),
        )
        .await
        .expect("ordinary download");

        assert_eq!(code, 0);
        let written = entries(out.path());
        assert_eq!(written.len(), 1, "exactly one file, found {written:?}");
        let body = std::fs::read(out.path().join(&written[0])).expect("read back");
        assert_eq!(body, vec![b'x'; 4096], "bytes must round-trip");
    }

    /// Exactly the ceiling is accepted end to end, and one byte more is refused, with the file
    /// system as the witness in both directions.
    #[tokio::test]
    async fn the_ceiling_boundary_holds_through_the_cli_path() {
        let out = tempfile::tempdir().expect("tmp");
        let exact = seeded_store("sha256:exact", 100).await;
        assert_eq!(
            pull_single(
                &exact,
                "sha256:exact",
                out.path(),
                false,
                StreamCeiling::new(100)
            )
            .await
            .expect("exact"),
            0
        );
        assert_eq!(entries(out.path()).len(), 1);

        let out2 = tempfile::tempdir().expect("tmp");
        let over = seeded_store("sha256:over", 101).await;
        assert_eq!(
            pull_single(
                &over,
                "sha256:over",
                out2.path(),
                false,
                StreamCeiling::new(100)
            )
            .await
            .expect("over"),
            2
        );
        assert!(entries(out2.path()).is_empty());
    }

    /// A missing bundle still exits 2 and writes nothing, unchanged by this slice.
    #[tokio::test]
    async fn a_missing_bundle_still_exits_two_and_writes_nothing() {
        let store = ObjectStoreBundleStore::memory();
        let out = tempfile::tempdir().expect("tmp");

        let code = pull_single(
            &store,
            "sha256:nonexistent",
            out.path(),
            false,
            StreamCeiling::new(100 * 1024),
        )
        .await
        .expect("not found is an exit code");

        assert_eq!(code, 2);
        assert!(entries(out.path()).is_empty());
    }
}

/// Untrusted identifiers reaching a terminal and a filesystem path.
///
/// A bundle id comes from the command line and a run id can come from a store listing; both are
/// echoed and one of them names a file. Neither was treated as untrusted.
#[cfg(test)]
mod untrusted_identifiers {
    use super::*;
    use assay_evidence::store::BundleStore;
    use assay_evidence::Bytes;

    /// The acceptance twin, and the shape every existing caller already sees. A well-formed id
    /// must keep the name it has always had, or this stops being a hardening change and becomes a
    /// rename that breaks whatever is looking for the file.
    #[test]
    fn a_well_formed_id_keeps_its_familiar_name() {
        assert_eq!(bundle_filename("sha256:abc"), "sha256_abc.tar.gz");
        assert_eq!(
            bundle_filename("sha256:0a1b2c3d4e5f"),
            "sha256_0a1b2c3d4e5f.tar.gz"
        );
    }

    /// The filename is always exactly one component. Asserting on the string alone would miss the
    /// point, so this asks the path type: after joining, is the parent still the directory the
    /// operator chose?
    #[test]
    fn no_id_can_name_a_file_outside_the_chosen_directory() {
        let out = Path::new("/tmp/assay-out");
        for hostile in [
            "../../etc/passwd",
            "../escape",
            "..",
            "/absolute/path",
            r"..\..\windows\system32\cfg",
            r"C:\Windows\System32\drivers",
            "nested/dir/file",
            "a/../../b",
            "\u{0}nul",
        ] {
            let name = bundle_filename(hostile);
            let joined = out.join(&name);

            assert_eq!(
                joined.parent(),
                Some(out),
                "id {hostile:?} produced {name:?}, which does not sit directly in the out dir"
            );
            assert_eq!(
                Path::new(&name).components().count(),
                1,
                "id {hostile:?} produced {name:?}, which is more than one component"
            );
            assert!(
                !name.contains('/') && !name.contains('\\'),
                "id {hostile:?} produced {name:?}, which still carries a separator"
            );
            assert!(name.ends_with(".tar.gz"), "{name:?}");
        }
    }

    /// Terminal control sequences never reach the filename either. `..` survives as text because
    /// `.` is a legitimate character in a name; what makes it harmless is that no separator does.
    #[test]
    fn the_filename_allowlist_admits_only_plain_ascii() {
        let name = bundle_filename("sha\u{1b}[31m256:\u{7}abc\u{9c}");
        assert_eq!(name, "sha__31m256__abc_.tar.gz");
        assert!(
            name.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')),
            "{name:?}"
        );
    }

    /// An id of nothing but rejected characters is still a name, and an empty one does not become
    /// a hidden file called `.tar.gz`.
    #[test]
    fn a_degenerate_id_still_yields_a_usable_name() {
        assert_eq!(bundle_filename(""), "bundle.tar.gz");
        assert_eq!(bundle_filename("///"), "___.tar.gz");
    }

    /// Escape sequences are stripped before anything is printed. The identifier itself is not
    /// changed by this — only what is written to the terminal.
    #[test]
    fn identifiers_are_stripped_before_they_reach_a_terminal() {
        let hostile = "sha256:abc\u{1b}[2K\u{1b}]0;pwned\u{7}";
        let rendered = shown(hostile);
        for control in ['\u{1b}', '\u{7}'] {
            assert!(
                !rendered.contains(control),
                "control character {control:?} survived into {rendered:?}"
            );
        }
        assert!(
            rendered.contains("sha256:abc"),
            "the readable part must survive: {rendered:?}"
        );
    }

    /// Sanitizing is for display only. The lookup uses the original string, because an id altered
    /// on the way to the store is a request for a different object — which would turn a hardening
    /// change into a silent wrong answer.
    #[tokio::test]
    async fn the_store_lookup_uses_the_unsanitized_id() {
        // A legitimate id that `bundle_filename` would rewrite, so a lookup keyed on the display
        // form would miss it.
        let id = "sha256:abc";
        let store = ObjectStoreBundleStore::memory();
        store
            .put_bundle(id, Bytes::from(vec![b'x'; 32]))
            .await
            .expect("seed");
        let out = tempfile::tempdir().expect("tmp");

        let code = pull_single(
            &store,
            id,
            out.path(),
            false,
            StreamCeiling::new(100 * 1024),
        )
        .await
        .expect("download");
        assert_eq!(code, 0, "the raw id must still resolve in the store");

        let written: Vec<String> = std::fs::read_dir(out.path())
            .expect("read dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(written, vec!["sha256_abc.tar.gz".to_string()]);
    }
}
