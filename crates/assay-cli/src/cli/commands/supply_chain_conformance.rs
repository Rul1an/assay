//! `assay registry supply-chain-conformance` (A5a-1): emit the `assay.supply_chain_conformance.v0`
//! carrier by running the existing `assay_registry::supply_chain::verify_supply_chain` producer over a
//! local, caller-supplied input descriptor.
//!
//! This is a thin CLI boundary around an existing producer. It introduces NO verifier/trust/policy
//! semantics. It performs OFFLINE checks over the supplied inputs and reports carrier status; it does
//! not assert supply-chain safety, policy approval, compliance, Sigstore trust, Rekor inclusion, issuer
//! identity, or artifact runtime integrity.
//!
//! v1 scope: `none` and `unsupported` provenance (these exercise the pinning, subject-digest, and
//! policy dimensions and need no cryptographic material). The signature-bearing provenance paths
//! (`dsse` pinned-key, `sigstore_bundle` keyless) are modeled in the descriptor but explicitly DEFERRED
//! to a follow-up: they are rejected with a clear non-zero, never silently ignored.

use std::io::Write;

use assay_registry::supply_chain::{
    verify_supply_chain, ContainerRef, PinningInput, Policy, ProvenanceInput, SlsaLevel, Subject,
    SupplyChainConformance, UnsupportedProvenance, VerifyInput,
};
use assay_registry::trust::TrustStore;
use serde::Deserialize;

use crate::cli::args::SupplyChainConformanceArgs;
use crate::exit_codes::{EXIT_CONFIG_ERROR, EXIT_INFRA_ERROR, EXIT_SUCCESS};

/// The CLI input descriptor schema. NOT a carrier, NOT a trust statement: a local mapping into
/// `VerifyInput`. Versioned so it can never become an implicit, unversioned format.
const INPUT_SCHEMA: &str = "assay.supply_chain_conformance.input.v0";

// ---- Input descriptor (strict / fail-closed: unknown fields are rejected, never ignored) --------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputDescriptor {
    schema: String,
    subject: SubjectInput,
    #[serde(default)]
    expected_artifact_digest: Option<String>,
    provenance: ProvenanceDescriptor,
    pinning: PinningDescriptor,
    policy: PolicyDescriptor,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SubjectInput {
    name: String,
    version: String,
    digest: String,
}

/// `kind` ∈ {none, unsupported, dsse, sigstore_bundle}. `format` is required iff kind==unsupported.
/// (Modeled as a struct rather than an internally-tagged enum to keep `deny_unknown_fields` robust.)
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProvenanceDescriptor {
    kind: String,
    #[serde(default)]
    format: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PinningDescriptor {
    version_pinned: bool,
    #[serde(default)]
    digest_pinned: Option<bool>,
    #[serde(default)]
    lockfile_digest: Option<String>,
    #[serde(default)]
    floating_source_ref: bool,
    /// "digest_pinned" | "tag_only" when present.
    #[serde(default)]
    container_ref: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyDescriptor {
    #[serde(default)]
    required_builder_id: Option<String>,
    #[serde(default)]
    required_slsa_build_level: u8,
    #[serde(default)]
    require_rekor_inclusion: bool,
    #[serde(default)]
    require_timestamp_freshness: bool,
    #[serde(default)]
    require_consistency: bool,
    #[serde(default)]
    require_witnessing: bool,
}

/// A descriptor error: the message is already operator-facing and the code is the process exit code.
#[derive(Debug)]
struct EmitErr {
    code: i32,
    msg: String,
}
fn contract(msg: impl Into<String>) -> EmitErr {
    EmitErr {
        code: EXIT_CONFIG_ERROR,
        msg: format!("[config_error] {}", msg.into()),
    }
}

// ---- Build the carrier from descriptor bytes (no file/stdout I/O — unit-testable) ---------------

fn build_carrier(raw: &str) -> Result<SupplyChainConformance, EmitErr> {
    let d: InputDescriptor = serde_json::from_str(raw)
        .map_err(|e| contract(format!("malformed input descriptor: {e}")))?;
    if d.schema != INPUT_SCHEMA {
        return Err(contract(format!(
            "input descriptor schema must be \"{INPUT_SCHEMA}\"; got {:?}",
            d.schema
        )));
    }

    let provenance = match d.provenance.kind.as_str() {
        "none" => {
            if d.provenance.format.is_some() {
                return Err(contract("provenance.format is only valid for kind \"unsupported\""));
            }
            ProvenanceInput::None
        }
        "unsupported" => {
            let format = d
                .provenance
                .format
                .as_deref()
                .ok_or_else(|| contract("provenance.format is required for kind \"unsupported\""))?;
            let kind = match format {
                "pep740" => UnsupportedProvenance::Pep740,
                "npm_provenance" => UnsupportedProvenance::NpmProvenance,
                "unknown_predicate" => UnsupportedProvenance::UnknownPredicate,
                other => {
                    return Err(contract(format!(
                        "unknown provenance.format {other:?} (expected pep740 | npm_provenance | unknown_predicate)"
                    )))
                }
            };
            ProvenanceInput::Unsupported(kind)
        }
        // Modeled but DEFERRED in v1 — rejected explicitly, never silently ignored.
        deferred @ ("dsse" | "sigstore_bundle") => {
            return Err(contract(format!(
                "provenance kind {deferred:?} is not yet wired into this emitter (v1 supports none | unsupported); a follow-up adds the signature-bearing paths"
            )))
        }
        other => {
            return Err(contract(format!(
                "unknown provenance.kind {other:?} (expected none | unsupported | dsse | sigstore_bundle)"
            )))
        }
    };

    let container_ref = match d.pinning.container_ref.as_deref() {
        None => None,
        Some("digest_pinned") => Some(ContainerRef::DigestPinned),
        Some("tag_only") => Some(ContainerRef::TagOnly),
        Some(other) => {
            return Err(contract(format!(
                "unknown pinning.container_ref {other:?} (expected digest_pinned | tag_only)"
            )))
        }
    };

    let pinning = PinningInput {
        version_pinned: d.pinning.version_pinned,
        digest_pinned: d.pinning.digest_pinned,
        lockfile_digest: d.pinning.lockfile_digest,
        floating_source_ref: d.pinning.floating_source_ref,
        container_ref,
    };

    let policy = Policy {
        required_builder_id: d.policy.required_builder_id,
        required_slsa_build_level: SlsaLevel(d.policy.required_slsa_build_level),
        require_rekor_inclusion: d.policy.require_rekor_inclusion,
        require_timestamp_freshness: d.policy.require_timestamp_freshness,
        require_consistency: d.policy.require_consistency,
        require_witnessing: d.policy.require_witnessing,
    };

    let subject = Subject {
        name: d.subject.name,
        version: d.subject.version,
        digest: d.subject.digest,
    };

    // v1 paths (none/unsupported) never consult the trust store; an empty store is correct.
    let trust_store = TrustStore::new();
    Ok(verify_supply_chain(VerifyInput {
        subject,
        expected_artifact_digest: d.expected_artifact_digest,
        provenance,
        pinning,
        policy,
        trust_store: &trust_store,
    }))
}

pub async fn run(args: SupplyChainConformanceArgs) -> anyhow::Result<i32> {
    // `--offline` is a guard, not a mode switch: the producer performs no network I/O by construction.
    // There is no fetch-capable path in this slice, so the guard is trivially satisfied; if one is ever
    // introduced it MUST hard-fail here before any fetch.
    let _ = args.offline;

    let raw = match std::fs::read_to_string(&args.input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "[config_error] cannot read input descriptor {}: {e}",
                args.input
            );
            return Ok(EXIT_CONFIG_ERROR);
        }
    };

    let carrier = match build_carrier(&raw) {
        Ok(c) => c,
        Err(EmitErr { code, msg }) => {
            eprintln!("{msg}");
            return Ok(code);
        }
    };

    let rendered = format!("{}\n", serde_json::to_string_pretty(&carrier)?);
    // Output-write failures are an infra/output problem regardless of the target: stdout and file
    // writes route through the same mapping, so a broken pipe on stdout is EXIT_INFRA_ERROR just like
    // an unwritable file path (never the generic `?` bubble).
    let write_result = if args.out == "-" {
        std::io::stdout().write_all(rendered.as_bytes())
    } else {
        std::fs::write(&args.out, &rendered)
    };
    let target = if args.out == "-" {
        "stdout"
    } else {
        args.out.as_str()
    };
    Ok(map_write_result(target, write_result))
}

/// Map an output-write result to an exit code. A write failure is an infra/output problem
/// (`EXIT_INFRA_ERROR`), applied uniformly to stdout and file targets so the exit-code contract is
/// the same whatever `--out` points at.
fn map_write_result(target: &str, result: std::io::Result<()>) -> i32 {
    match result {
        Ok(()) => EXIT_SUCCESS,
        Err(e) => {
            eprintln!("[infra_error] cannot write output ({target}): {e}");
            EXIT_INFRA_ERROR
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assay_registry::supply_chain::SCHEMA;
    use serde_json::{json, Value};

    fn descriptor(provenance: Value) -> String {
        json!({
            "schema": INPUT_SCHEMA,
            "subject": { "name": "demo", "version": "1.0.0", "digest": "sha256:aa" },
            "provenance": provenance,
            "pinning": { "version_pinned": true, "digest_pinned": true },
            "policy": { "required_slsa_build_level": 0 }
        })
        .to_string()
    }
    fn carrier_value(raw: &str) -> Value {
        serde_json::to_value(build_carrier(raw).expect("carrier")).expect("value")
    }

    #[test]
    fn none_provenance_emits_a_valid_carrier() {
        let v = carrier_value(&descriptor(json!({ "kind": "none" })));
        assert_eq!(v["schema"], SCHEMA);
        // provenance is absent, not verified as trusted.
        assert_eq!(v["checks"]["provenance"]["slsa_provenance"], "not_present");
    }

    #[test]
    fn unsupported_provenance_records_unsupported_format() {
        let v = carrier_value(&descriptor(
            json!({ "kind": "unsupported", "format": "pep740" }),
        ));
        assert_eq!(v["schema"], SCHEMA);
        assert_eq!(
            v["checks"]["provenance"]["slsa_provenance"],
            "unsupported_format"
        );
    }

    #[test]
    fn expected_artifact_digest_mismatch_fails_the_integrity_check() {
        // expected_artifact_digest != subject.digest -> integrity.artifact_digest = failed (real check).
        let mut d: Value = serde_json::from_str(&descriptor(json!({ "kind": "none" }))).unwrap();
        d["expected_artifact_digest"] = json!("sha256:bb"); // subject.digest is sha256:aa
        let v = carrier_value(&d.to_string());
        assert_eq!(v["checks"]["integrity"]["artifact_digest"], "failed");
    }

    #[test]
    fn matching_expected_artifact_digest_verifies_the_integrity_check() {
        let mut d: Value = serde_json::from_str(&descriptor(json!({ "kind": "none" }))).unwrap();
        d["expected_artifact_digest"] = json!("sha256:aa"); // == subject.digest
        let v = carrier_value(&d.to_string());
        assert_eq!(v["checks"]["integrity"]["artifact_digest"], "verified");
    }

    #[test]
    fn wrong_input_schema_is_rejected() {
        let mut d: Value = serde_json::from_str(&descriptor(json!({ "kind": "none" }))).unwrap();
        d["schema"] = json!("assay.supply_chain_conformance.v0"); // the carrier schema, not the input schema
        assert_eq!(
            build_carrier(&d.to_string()).err().unwrap().code,
            EXIT_CONFIG_ERROR
        );
    }

    #[test]
    fn unknown_top_level_field_is_rejected() {
        let mut d: Value = serde_json::from_str(&descriptor(json!({ "kind": "none" }))).unwrap();
        d["trust_me_bro"] = json!(true);
        assert!(build_carrier(&d.to_string()).is_err());
    }

    #[test]
    fn unknown_provenance_kind_is_rejected() {
        assert!(build_carrier(&descriptor(json!({ "kind": "weird" }))).is_err());
    }

    #[test]
    fn unsupported_without_format_is_rejected() {
        assert!(build_carrier(&descriptor(json!({ "kind": "unsupported" }))).is_err());
    }

    #[test]
    fn unknown_unsupported_format_is_rejected() {
        assert!(build_carrier(&descriptor(
            json!({ "kind": "unsupported", "format": "made_up" })
        ))
        .is_err());
    }

    #[test]
    fn deferred_dsse_is_rejected_not_ignored() {
        let e = build_carrier(&descriptor(json!({ "kind": "dsse" })))
            .err()
            .unwrap();
        assert_eq!(e.code, EXIT_CONFIG_ERROR);
        assert!(e.msg.contains("not yet wired"));
    }

    #[test]
    fn deferred_sigstore_bundle_is_rejected_not_ignored() {
        assert!(build_carrier(&descriptor(json!({ "kind": "sigstore_bundle" }))).is_err());
    }

    #[test]
    fn write_failure_maps_to_infra_error_for_any_target() {
        use std::io::{Error, ErrorKind};
        // A write failure is EXIT_INFRA_ERROR uniformly — stdout (broken pipe) and file alike.
        assert_eq!(
            map_write_result(
                "stdout",
                Err(Error::new(ErrorKind::BrokenPipe, "pipe closed"))
            ),
            EXIT_INFRA_ERROR
        );
        assert_eq!(
            map_write_result(
                "/tmp/x.json",
                Err(Error::new(ErrorKind::PermissionDenied, "nope"))
            ),
            EXIT_INFRA_ERROR
        );
        assert_eq!(map_write_result("stdout", Ok(())), EXIT_SUCCESS);
    }

    #[test]
    fn unknown_container_ref_is_rejected() {
        let mut d: Value = serde_json::from_str(&descriptor(json!({ "kind": "none" }))).unwrap();
        d["pinning"]["container_ref"] = json!("floating");
        assert!(build_carrier(&d.to_string()).is_err());
    }
}
