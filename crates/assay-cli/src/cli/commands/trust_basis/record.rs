//! Build the report and its `assay.trust-basis.inputs.v0` record from one generation.
//!
//! Everything the sidecar states is taken from what generation actually consumed: the
//! bytes the opened bundle reader returned, the limits value passed in, and the packs
//! as loaded, in order and with duplicates. The pair is checked with the reader's own
//! validator before the caller creates any output.

use super::capture::CaptureRead;
use super::inputs::{
    render_pretty, validate_pair, ByteIdentityV0, InputsV0, LimitsV0, LintV0, PackV0, PairRefusal,
    INPUTS_SCHEMA, PACK_DIGEST_DOMAIN,
};
use super::publish::Publication;
use assay_evidence::lint::engine::LintOptions;
use assay_evidence::lint::packs::{LoadedPack, PackSource};
use assay_evidence::{
    generate_trust_basis, to_canonical_json_bytes, TrustBasisOptions, VerifyLimits,
};
use std::io::Read;

#[derive(Debug)]
pub(super) enum RecordError {
    /// Generation itself failed: the same failure the legacy outputs report.
    Generation(anyhow::Error),
    /// Generation returned without the bundle reader reaching end of file.
    CaptureIncomplete,
    /// The report or the record does not render.
    Render,
    /// The pair this build produced does not pass the reader's validator.
    Unrepresentable(PairRefusal),
}

impl RecordError {
    /// A fixed description; nothing from the bundle or a pack is echoed.
    pub(super) fn describe(&self) -> String {
        match self {
            Self::Generation(_) => "generation failed".to_string(),
            Self::CaptureIncomplete => "the bundle reader did not reach end of file".to_string(),
            Self::Render => "a document did not render".to_string(),
            Self::Unrepresentable(refusal) => {
                format!(
                    "the pair does not pass the inputs validator ({})",
                    refusal.code()
                )
            }
        }
    }
}

/// The lint options generation runs with, shared by every output mode.
pub(super) fn lint_options(
    packs: Option<Vec<LoadedPack>>,
    max_results: usize,
    bundle_label: String,
) -> Option<LintOptions> {
    packs.map(|packs| LintOptions {
        packs,
        max_results: Some(max_results),
        bundle_path: Some(bundle_label),
    })
}

fn pack_record(pack: &LoadedPack) -> PackV0 {
    PackV0 {
        name: pack.definition.name.clone(),
        version: pack.definition.version.clone(),
        source_kind: match pack.source {
            PackSource::BuiltIn(_) => "builtin",
            PackSource::File(_) => "file",
        }
        .to_string(),
        digest: pack.digest.clone(),
    }
}

/// Generate from `bundle` and build the validated pair.
pub(super) fn build_publication<R: Read>(
    bundle: R,
    limits: VerifyLimits,
    lint: Option<LintOptions>,
) -> Result<Publication, RecordError> {
    // Snapshot before generation takes ownership of the options.
    let limits_record = LimitsV0::from_limits(&limits);
    let packs: Vec<PackV0> = lint
        .as_ref()
        .map(|lint| lint.packs.iter().map(pack_record).collect())
        .unwrap_or_default();
    let lint_record = match &lint {
        Some(options) if !options.packs.is_empty() => LintV0 {
            enabled: true,
            max_results: options.max_results.map(|cap| cap as u64),
        },
        _ => LintV0 {
            enabled: false,
            max_results: None,
        },
    };

    let mut capture = CaptureRead::new(bundle);
    let trust_basis = generate_trust_basis(&mut capture, limits, TrustBasisOptions { lint })
        .map_err(RecordError::Generation)?;
    let bundle_identity = capture.finish().ok_or(RecordError::CaptureIncomplete)?;

    let report = to_canonical_json_bytes(&trust_basis).map_err(|_| RecordError::Render)?;
    let inputs = InputsV0 {
        schema: INPUTS_SCHEMA.to_string(),
        report: ByteIdentityV0::of(&report),
        bundle: bundle_identity,
        pack_digest_domain: PACK_DIGEST_DOMAIN.to_string(),
        packs,
        limits: limits_record,
        lint: lint_record,
        reported_assay_version: env!("CARGO_PKG_VERSION").to_string(),
    };
    let sidecar = render_pretty(&inputs).map_err(|_| RecordError::Render)?;
    validate_pair(&report, &sidecar).map_err(RecordError::Unrepresentable)?;
    Ok(Publication { report, sidecar })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::commands::trust_basis::inputs::{validate_inputs, Refusal, MAX_WIRE_INTEGER};
    use assay_evidence::lint::packs::load_pack;
    use assay_evidence::{BundleWriter, EvidenceEvent};
    use chrono::{TimeZone, Utc};
    use sha2::{Digest, Sha256};
    use std::io::{self, Cursor};

    const PACK: &str = "owasp-agentic-a3-a5-signal-followup";

    fn bundle_bytes() -> Vec<u8> {
        let mut event = EvidenceEvent::new(
            "assay.tool.decision",
            "urn:assay:test:trust-basis-record",
            "run_record",
            0,
            serde_json::json!({ "tool": "tool.commit", "decision": "allow" }),
        );
        event.time = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let mut bytes = Vec::new();
        let mut writer = BundleWriter::new(&mut bytes);
        writer.add_event(event);
        writer.finish().unwrap();
        bytes
    }

    fn build(limits: VerifyLimits, lint: Option<LintOptions>) -> Result<InputsV0, RecordError> {
        let bundle = bundle_bytes();
        let publication = build_publication(Cursor::new(&bundle), limits, lint)?;
        let inputs =
            validate_inputs(&publication.sidecar).expect("the writer emits valid sidecars");
        assert_eq!(inputs.bundle.sha256, hex::encode(Sha256::digest(&bundle)));
        assert_eq!(inputs.bundle.bytes, bundle.len() as u64);
        assert_eq!(
            inputs.report.sha256,
            hex::encode(Sha256::digest(&publication.report))
        );
        Ok(inputs)
    }

    fn lint_with(packs: Vec<LoadedPack>, max_results: usize) -> Option<LintOptions> {
        lint_options(Some(packs), max_results, "bundle.tar.gz".to_string())
    }

    #[test]
    fn nondefault_limits_are_recorded_as_passed_not_as_defaults() {
        let limits = VerifyLimits {
            max_bundle_bytes: 1_000_001,
            max_decode_bytes: 2_000_003,
            max_manifest_bytes: 300_007,
            max_events_bytes: 400_009,
            max_events: 11,
            max_line_bytes: 50_013,
            max_path_len: 217,
            max_json_depth: 33,
        };
        let inputs = build(limits, None).unwrap();
        assert_eq!(
            inputs.limits,
            LimitsV0 {
                max_bundle_bytes: 1_000_001,
                max_decode_bytes: 2_000_003,
                max_manifest_bytes: 300_007,
                max_events_bytes: 400_009,
                max_events: 11,
                max_line_bytes: 50_013,
                max_path_len: 217,
                max_json_depth: 33,
            }
        );
    }

    #[test]
    fn a_limit_beyond_the_wire_range_fails_before_any_output_exists() {
        let limits = VerifyLimits {
            max_decode_bytes: MAX_WIRE_INTEGER + 1,
            ..VerifyLimits::for_retained_events()
        };
        // `build_publication` is the only producer of a `Publication`, so a refusal
        // here means nothing reaches the directory writer.
        match build(limits, None) {
            Err(RecordError::Unrepresentable(PairRefusal::Sidecar(Refusal::NumberInvalid))) => {}
            other => panic!("expected a sidecar number refusal, got {other:?}"),
        }
        let at_edge = VerifyLimits {
            max_decode_bytes: MAX_WIRE_INTEGER,
            ..VerifyLimits::for_retained_events()
        };
        assert_eq!(
            build(at_edge, None).unwrap().limits.max_decode_bytes,
            MAX_WIRE_INTEGER
        );
    }

    #[test]
    fn packs_keep_load_order_and_duplicates() {
        let builtin = load_pack(PACK).unwrap();
        let other = load_pack("mcp-signal-followup").unwrap();
        let names = |inputs: &InputsV0| -> Vec<String> {
            inputs.packs.iter().map(|pack| pack.name.clone()).collect()
        };
        let inputs = build(
            VerifyLimits::for_retained_events(),
            lint_with(vec![other.clone(), builtin.clone(), other.clone()], 500),
        )
        .unwrap();
        let other_name = other.definition.name.clone();
        assert_eq!(
            names(&inputs),
            [other_name.clone(), PACK.to_string(), other_name]
        );
        assert_eq!(inputs.packs[0], inputs.packs[2]);
        assert_eq!(inputs.packs[1].digest, builtin.digest);
        assert!(inputs
            .packs
            .iter()
            .all(|pack| pack.source_kind == "builtin"));
        assert_eq!(
            inputs.lint,
            LintV0 {
                enabled: true,
                max_results: Some(500)
            }
        );
    }

    #[test]
    fn an_empty_pack_list_records_lint_as_not_run() {
        // Generation skips lint for an empty list, so the record must too.
        let inputs = build(VerifyLimits::for_retained_events(), lint_with(vec![], 7)).unwrap();
        assert!(inputs.packs.is_empty());
        assert_eq!(
            inputs.lint,
            LintV0 {
                enabled: false,
                max_results: None
            }
        );
    }

    #[test]
    fn a_zero_result_cap_is_recorded_as_enabled() {
        let inputs = build(
            VerifyLimits::for_retained_events(),
            lint_with(vec![load_pack(PACK).unwrap()], 0),
        )
        .unwrap();
        assert_eq!(
            inputs.lint,
            LintV0 {
                enabled: true,
                max_results: Some(0)
            }
        );
    }

    #[test]
    fn a_read_error_is_a_generation_failure_and_keeps_its_kind() {
        struct Failing;
        impl Read for Failing {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied"))
            }
        }
        match build_publication(Failing, VerifyLimits::for_retained_events(), None) {
            Err(RecordError::Generation(error)) => {
                let io = error
                    .downcast_ref::<io::Error>()
                    .expect("io error preserved");
                assert_eq!(io.kind(), io::ErrorKind::PermissionDenied);
            }
            Err(other) => panic!("expected a generation failure, got {other:?}"),
            Ok(_) => panic!("expected a generation failure"),
        }
    }

    #[test]
    fn the_report_is_serialized_once_and_matches_the_legacy_renderer() {
        let bundle = bundle_bytes();
        let publication = build_publication(
            Cursor::new(&bundle),
            VerifyLimits::for_retained_events(),
            None,
        )
        .unwrap();
        let legacy = to_canonical_json_bytes(
            &generate_trust_basis(
                Cursor::new(&bundle),
                VerifyLimits::for_retained_events(),
                TrustBasisOptions { lint: None },
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(publication.report, legacy);
    }
}
