//! `assay evidence attest` — sign a bundle's manifest as an in-toto/DSSE attestation.
//!
//! Wraps `assay_evidence::attestation` (ADR-039): opens and verifies an evidence
//! bundle, builds an in-toto v1 Statement over its integrity root, and signs it
//! as a DSSE envelope with an Ed25519 key (PKCS#8 PEM, as produced by
//! `assay mcp tool keygen`). The anchor (transparency log / timestamp) stays
//! external. Attestation binds who-said-it and the bundle content; it does not
//! upgrade observed support.

use crate::output_write::write_stdout_json;
use anyhow::{Context, Result};
use assay_common::limits::{LimitKind, LimitReader};
use assay_evidence::attestation::{sign_statement, statement_for_bundle_with_limits};
use assay_evidence::VerifyLimits;
use clap::Args;
use ed25519_dalek::pkcs8::DecodePrivateKey;
use ed25519_dalek::SigningKey;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

#[derive(Debug, Args, Clone)]
pub struct AttestArgs {
    /// Path to the evidence bundle (.tar.gz) to attest.
    #[arg(long)]
    pub bundle: PathBuf,
    /// Path to the Ed25519 private key (PKCS#8 PEM; see `assay mcp tool keygen`).
    #[arg(long)]
    pub key: PathBuf,
    /// Not accepted: the evidence-bundle/v1 predicate is derived from the bundle so a consumer
    /// can cross-check it against the artifact (ADR-044). Passing this flag is an error.
    #[arg(long)]
    pub predicate: Option<PathBuf>,
    /// Write the DSSE envelope here (default: stdout).
    #[arg(long)]
    pub out: Option<PathBuf>,
}

/// Read a bundle file, never taking in more than the source ceiling allows.
///
/// A named function so the ceiling can be exercised with a small limit. Inline, the only way to
/// prove it bit was to hand the command a file larger than the 100 MB default, which is why the
/// equivalent bound elsewhere in this repository went untested for as long as it did.
fn read_bundle_bounded(path: &std::path::Path, limits: VerifyLimits) -> Result<Vec<u8>> {
    let mut file = File::open(path).with_context(|| format!("open bundle {}", path.display()))?;
    let mut bytes = Vec::new();
    LimitReader::new(&mut file, limits.max_bundle_bytes, LimitKind::SourceBytes)
        .read_to_end(&mut bytes)
        .with_context(|| format!("read bundle {}", path.display()))?;
    Ok(bytes)
}

pub fn cmd_attest(args: AttestArgs) -> Result<i32> {
    run(args)
}

/// Returns the process exit rather than `()`.
///
/// The envelope is a machine document, and whether it was delivered is an exit class, not a detail
/// of the value produced. `Result<()>` had nowhere to put that, so a failed stdout write could only
/// become an `Err` — which reaches the generic top-level fallback and reports the config-error exit
/// `2` for what is an output-write failure (#2441). Returning the writer's own code keeps the
/// registered infrastructure exit `3` intact and leaves `Err` meaning what it already meant here:
/// the attestation itself could not be produced.
fn run(args: AttestArgs) -> Result<i32> {
    // 1. Refuse an unusable invocation before touching anything on disk.
    //
    //    `--predicate` is rejected rather than silently ignored: every v1 field is derived from the
    //    bundle so a consumer can cross-check it against the artifact, and a predicate the caller
    //    writes by hand cannot offer that.
    //
    //    Ordered first because the operator should learn what is wrong with their command, not
    //    what is wrong with their filesystem. Checked after the reads, a bad path masked the flag
    //    error entirely, so someone debugging a missing bundle would fix the path and only then
    //    discover their predicate was never going to be used.
    if args.predicate.is_some() {
        anyhow::bail!(
            "--predicate is not accepted for evidence-bundle/v1: every field is derived from the \
             bundle so a consumer can cross-check it against the artifact (ADR-044)"
        );
    }

    // 2. Read the bundle through the source ceiling, before anything is materialized.
    //
    //    ADR-043 §1: the limit applies to the stream. `std::fs::read` sized the allocation from
    //    the file, so an oversized archive was already in memory by the time `max_bundle_bytes`
    //    could have an opinion — the same class this repository closed for `evidence push`, left
    //    open here because this path reads the bundle for a different reason.
    //
    //    `args.bundle` is a user-selected path by contract: this subcommand exists to attest a
    //    bundle the operator names, and the key and output paths are theirs too. Containing them
    //    under the repository root would change the published interface to satisfy a taint
    //    tracker, so the path stays operator-chosen and the exposure that matters — how much of it
    //    is read — is bounded here instead.
    let limits = VerifyLimits::default();
    let bundle_bytes = read_bundle_bounded(&args.bundle, limits)?;

    // 3. Load the Ed25519 signing key (PKCS#8 PEM).
    let pem = std::fs::read_to_string(&args.key)
        .with_context(|| format!("read key {}", args.key.display()))?;
    let key = SigningKey::from_pkcs8_pem(&pem).context("parse Ed25519 PKCS#8 PEM key")?;

    // 4. Build the statement from the bounded bytes. Verification, predicate derivation and the
    //    subject name all happen inside that one call, against these bytes, so the CLI has no way
    //    to pair a predicate with an artifact it does not describe.
    let statement = statement_for_bundle_with_limits(&bundle_bytes, limits)?;
    let envelope = sign_statement(&statement, &key).context("sign in-toto statement")?;
    let json = serde_json::to_string_pretty(&envelope).context("serialize DSSE envelope")?;

    // 5. Write the DSSE envelope.
    match &args.out {
        Some(p) => {
            std::fs::write(p, format!("{json}\n"))
                .with_context(|| format!("write {}", p.display()))?;
            eprintln!("Attestation: {}", p.display());
        }
        None => return Ok(write_stdout_json(&json)),
    }
    Ok(crate::exit_codes::EXIT_SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use assay_evidence::attestation::{verify_envelope_signature, DsseEnvelope};
    use assay_evidence::bundle::BundleWriter;
    use assay_evidence::types::{EvidenceEvent, ProducerMeta};
    use ed25519_dalek::pkcs8::{spki::der::pem::LineEnding, EncodePrivateKey};

    struct Fixture {
        dir: PathBuf,
        bundle: PathBuf,
        key: PathBuf,
        out: PathBuf,
        signing: SigningKey,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.dir).ok();
        }
    }

    fn fixture(tag: &str) -> Fixture {
        let dir =
            std::env::temp_dir().join(format!("assay-attest-cli-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bundle = dir.join("bundle.tar.gz");
        let key = dir.join("private_key.pem");
        let out = dir.join("attestation.json");

        let producer = ProducerMeta {
            name: "assay-cli".into(),
            version: "test".into(),
            git: None,
        };
        let file = File::create(&bundle).unwrap();
        let mut writer = BundleWriter::new(file).with_producer(producer.clone());
        writer.add_event(
            EvidenceEvent::new(
                "assay.test.event",
                "urn:assay:test",
                "attest_run",
                0,
                serde_json::json!({}),
            )
            .with_producer(&producer),
        );
        writer.finish().unwrap();

        let signing = SigningKey::from_bytes(&[7u8; 32]);
        std::fs::write(
            &key,
            signing.to_pkcs8_pem(LineEnding::LF).unwrap().as_bytes(),
        )
        .unwrap();

        Fixture {
            dir,
            bundle,
            key,
            out,
            signing,
        }
    }

    #[test]
    fn attest_produces_an_envelope_that_verifies_against_the_bundle() {
        let f = fixture("happy");
        run(AttestArgs {
            bundle: f.bundle.clone(),
            key: f.key.clone(),
            predicate: None,
            out: Some(f.out.clone()),
        })
        .expect("attest");

        let raw = std::fs::read_to_string(&f.out).unwrap();
        let envelope: DsseEnvelope = serde_json::from_str(&raw).unwrap();

        // Signature alone is a state, not a verdict: it says who signed, not which artifact.
        let signed = verify_envelope_signature(&envelope, &f.signing.verifying_key()).expect("sig");
        assert_eq!(signed.statement.type_, "https://in-toto.io/Statement/v1");

        // The verdict needs the bytes. Read through the command's own bounded reader rather than
        // `std::fs::read`: the test path is test-authored, but reaching for the unbounded call
        // here is what let the production path keep one for so long.
        let bytes = read_bundle_bounded(&f.bundle, VerifyLimits::default()).expect("read");
        let attested = assay_evidence::attestation::verify_attestation_for_bundle(
            &envelope,
            &f.signing.verifying_key(),
            &bytes,
        )
        .expect("attestation must verify against the bundle it describes");
        assert_eq!(attested.predicate.run.event_count, 1);

        let mut other = bytes.clone();
        other.extend_from_slice(b"trailing");
        let err = assay_evidence::attestation::verify_attestation_for_bundle(
            &envelope,
            &f.signing.verifying_key(),
            &other,
        )
        .expect_err("a different artifact must not match");
        assert!(err.to_string().contains("does not match"), "got: {err}");
    }

    /// `--predicate` is refused, not ignored.
    ///
    /// Silently dropping it would leave the operator believing a predicate they wrote was signed.
    #[test]
    fn an_explicit_predicate_is_refused() {
        let f = fixture("predicate");
        let predicate = f.dir.join("predicate.json");
        std::fs::write(&predicate, "{}").unwrap();

        let err = run(AttestArgs {
            bundle: f.bundle.clone(),
            key: f.key.clone(),
            predicate: Some(predicate),
            out: Some(f.out.clone()),
        })
        .expect_err("--predicate must be refused");
        assert!(err.to_string().contains("--predicate"), "got: {err}");
        assert!(!f.out.exists(), "nothing may be signed after a refusal");
    }

    /// The refusal comes before any file is touched, and the paths prove the order.
    ///
    /// Both paths do not exist. If the flag were checked after the bundle read, the command would
    /// fail on the missing file instead — a different refusal, for a different reason, that a test
    /// asserting only `is_err()` would happily accept. The operator gets told what is wrong with
    /// their invocation rather than what is wrong with their filesystem.
    #[test]
    fn the_predicate_flag_is_refused_before_any_file_is_touched() {
        let f = fixture("order");
        let predicate = f.dir.join("predicate.json");
        std::fs::write(&predicate, "{}").unwrap();

        let err = run(AttestArgs {
            bundle: f.dir.join("no-such-bundle.tar.gz"),
            key: f.dir.join("no-such-key.pem"),
            predicate: Some(predicate),
            out: Some(f.out.clone()),
        })
        .expect_err("--predicate must be refused");

        assert!(
            err.to_string().contains("--predicate"),
            "the refusal must be about the flag, got: {err}"
        );
        assert!(
            !err.to_string().contains("no-such-bundle"),
            "the bundle must not have been opened, got: {err}"
        );
        assert!(!f.out.exists(), "nothing may be written after a refusal");
    }

    /// The help text must not promise behaviour the command refuses.
    #[test]
    fn the_predicate_help_does_not_advertise_a_default() {
        use clap::Args as _;
        let help = AttestArgs::augment_args(clap::Command::new("attest"))
            .render_long_help()
            .to_string();
        assert!(
            !help.contains("default: a minimal summary"),
            "the help still advertises a default for a flag that is rejected:\n{help}"
        );
        // Case-insensitive: the claim is about what the help says, not how it is capitalised.
        assert!(
            help.to_lowercase().contains("not accepted"),
            "the help must say the flag is rejected:\n{help}"
        );
    }

    /// The source ceiling applies to the read itself, before the bundle is materialized.
    #[test]
    fn the_source_ceiling_bounds_the_read() {
        let f = fixture("ceiling");
        let tiny = VerifyLimits {
            max_bundle_bytes: 8,
            ..VerifyLimits::default()
        };

        let err =
            read_bundle_bounded(&f.bundle, tiny).expect_err("must refuse an oversized source");
        let chain = format!("{err:#}");
        assert!(
            chain.contains("source bytes limit"),
            "expected the source ceiling to be the reason, got: {chain}"
        );

        // And the same file passes under the real ceiling, so the case is about the limit rather
        // than about the file being unreadable.
        assert!(read_bundle_bounded(&f.bundle, VerifyLimits::default()).is_ok());
    }
}
