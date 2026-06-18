//! `assay registry supply-chain-conformance` (A5a-1): emit the `assay.supply_chain_conformance.v0`
//! carrier by running the existing `assay_registry::supply_chain::verify_supply_chain` producer over a
//! local, caller-supplied input descriptor.
//!
//! This is a thin CLI boundary around an existing producer. It introduces NO verifier/trust/policy
//! semantics. It performs OFFLINE checks over the supplied inputs and reports carrier status; it does
//! not assert supply-chain safety, policy approval, compliance, Sigstore trust, Rekor inclusion, issuer
//! identity, or artifact runtime integrity.
//!
//! Scope: `none`, `unsupported`, and `dsse` (pinned-key, offline) provenance. The `dsse` path verifies a
//! local DSSE-wrapped in-toto/SLSA statement against a caller-supplied pinned Ed25519 key via the existing
//! `assay_registry` verifier — NO cryptography is implemented here, only descriptor->VerifyInput wiring and
//! safe descriptor-relative file resolution. The keyless `sigstore_bundle` path is modeled in the descriptor
//! but explicitly DEFERRED: it is rejected with a clear non-zero, never silently ignored.

use std::io::Write;
use std::path::{Path, PathBuf};

use assay_registry::supply_chain::{
    verify_supply_chain, ContainerRef, PinningInput, Policy, ProvenanceInput, SlsaLevel, Subject,
    SupplyChainConformance, UnsupportedProvenance, VerifyInput,
};
use assay_registry::trust::TrustStore;
use assay_registry::types::{DsseEnvelope, TrustedKey};
use serde::Deserialize;

use crate::cli::args::SupplyChainConformanceArgs;
use crate::exit_codes::{EXIT_CONFIG_ERROR, EXIT_INFRA_ERROR, EXIT_SUCCESS};

/// The CLI input descriptor schema. NOT a carrier, NOT a trust statement: a local mapping into
/// `VerifyInput`. Versioned so it can never become an implicit, unversioned format.
const INPUT_SCHEMA: &str = "assay.supply_chain_conformance.input.v0";

/// The only DSSE payload type this slice accepts (an in-toto Statement). The CLI binds it; the
/// `assay_registry` verifier re-checks it (a mismatch there yields `dsse_signature: unsupported_format`).
const DSSE_IN_TOTO_PAYLOAD_TYPE: &str = "application/vnd.in-toto+json";

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
/// `payload_type` / `envelope_path` / `trusted_key_path` are required iff kind==dsse, and rejected on any
/// other kind (never silently ignored). Paths are resolved relative to the descriptor file (see
/// `resolve_under`). (Modeled as a struct rather than an internally-tagged enum to keep
/// `deny_unknown_fields` robust.)
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProvenanceDescriptor {
    kind: String,
    #[serde(default)]
    format: Option<String>,
    /// dsse: the DSSE envelope's `payloadType` (must be `application/vnd.in-toto+json`).
    #[serde(default)]
    payload_type: Option<String>,
    /// dsse: descriptor-relative path to the DSSE envelope JSON.
    #[serde(default)]
    envelope_path: Option<String>,
    /// dsse: descriptor-relative path to the pinned `TrustedKey` JSON.
    #[serde(default)]
    trusted_key_path: Option<String>,
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

// ---- Safe descriptor-relative path resolution (the only new I/O surface in this slice) ----------

/// Resolve a descriptor-relative path under `base_dir`, fail-closed. Rejects URLs, absolute paths, `..`
/// components, and control chars; requires the canonicalized target to be a regular file that stays under
/// the (canonicalized) descriptor directory, so a symlink cannot escape it. Mirrors the evidence
/// baseline-key path discipline. Any violation is an `EXIT_CONFIG_ERROR` (operator input problem).
fn resolve_under(base_dir: &Path, rel: &str, field: &str) -> Result<PathBuf, EmitErr> {
    if rel.is_empty() {
        return Err(contract(format!("provenance.{field} must not be empty")));
    }
    if rel.contains("://") {
        return Err(contract(format!(
            "provenance.{field} {rel:?} looks like a URL; only descriptor-relative files are allowed"
        )));
    }
    if rel.chars().any(|c| c.is_control()) {
        return Err(contract(format!(
            "provenance.{field} contains control characters"
        )));
    }
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        return Err(contract(format!(
            "provenance.{field} {rel:?} must be descriptor-relative, not absolute"
        )));
    }
    if rel_path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(contract(format!(
            "provenance.{field} {rel:?} must not contain \"..\""
        )));
    }
    let base = base_dir
        .canonicalize()
        .map_err(|e| contract(format!("cannot resolve descriptor directory: {e}")))?;
    let full = base
        .join(rel_path)
        .canonicalize()
        .map_err(|e| contract(format!("cannot resolve provenance.{field} {rel:?}: {e}")))?;
    if !full.starts_with(&base) {
        return Err(contract(format!(
            "provenance.{field} {rel:?} escapes the descriptor directory"
        )));
    }
    if !full.is_file() {
        return Err(contract(format!(
            "provenance.{field} {rel:?} is not a regular file"
        )));
    }
    Ok(full)
}

/// The dsse-only descriptor fields must not appear on any other kind (else they would be silently
/// ignored, which `deny_unknown_fields` cannot catch since the fields are valid on the struct).
fn reject_dsse_fields(p: &ProvenanceDescriptor, kind: &str) -> Result<(), EmitErr> {
    if p.payload_type.is_some() || p.envelope_path.is_some() || p.trusted_key_path.is_some() {
        return Err(contract(format!(
            "provenance.payload_type/envelope_path/trusted_key_path are only valid for kind \"dsse\", not {kind:?}"
        )));
    }
    Ok(())
}

// ---- Build the carrier from descriptor bytes (file I/O only for the dsse path, under base_dir) ---

fn build_carrier(raw: &str, base_dir: &Path) -> Result<SupplyChainConformance, EmitErr> {
    let d: InputDescriptor = serde_json::from_str(raw)
        .map_err(|e| contract(format!("malformed input descriptor: {e}")))?;
    if d.schema != INPUT_SCHEMA {
        return Err(contract(format!(
            "input descriptor schema must be \"{INPUT_SCHEMA}\"; got {:?}",
            d.schema
        )));
    }

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

    // The trust store must outlive the verify call. none/unsupported never consult it (empty store); the
    // dsse arm replaces it with one pinning the caller-supplied key.
    let store;
    let provenance = match d.provenance.kind.as_str() {
        "none" => {
            if d.provenance.format.is_some() {
                return Err(contract("provenance.format is only valid for kind \"unsupported\""));
            }
            reject_dsse_fields(&d.provenance, "none")?;
            store = TrustStore::new();
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
            reject_dsse_fields(&d.provenance, "unsupported")?;
            store = TrustStore::new();
            ProvenanceInput::Unsupported(kind)
        }
        // Pinned-key DSSE: verified offline by the existing `assay_registry` verifier. We only wire the
        // descriptor -> envelope/key -> VerifyInput; the crypto, PAE, in-toto parse, and policy live there.
        "dsse" => {
            if d.provenance.format.is_some() {
                return Err(contract("provenance.format is only valid for kind \"unsupported\""));
            }
            let payload_type = d
                .provenance
                .payload_type
                .as_deref()
                .ok_or_else(|| contract("provenance.payload_type is required for kind \"dsse\""))?;
            let envelope_path = d
                .provenance
                .envelope_path
                .as_deref()
                .ok_or_else(|| contract("provenance.envelope_path is required for kind \"dsse\""))?;
            let trusted_key_path = d
                .provenance
                .trusted_key_path
                .as_deref()
                .ok_or_else(|| contract("provenance.trusted_key_path is required for kind \"dsse\""))?;
            if payload_type != DSSE_IN_TOTO_PAYLOAD_TYPE {
                return Err(contract(format!(
                    "provenance.payload_type must be {DSSE_IN_TOTO_PAYLOAD_TYPE:?} for kind \"dsse\"; got {payload_type:?}"
                )));
            }
            let env_file = resolve_under(base_dir, envelope_path, "envelope_path")?;
            let env_bytes = std::fs::read(&env_file).map_err(|e| {
                contract(format!(
                    "cannot read provenance.envelope_path {envelope_path:?}: {e}"
                ))
            })?;
            let envelope: DsseEnvelope = serde_json::from_slice(&env_bytes)
                .map_err(|e| contract(format!("malformed DSSE envelope {envelope_path:?}: {e}")))?;
            if envelope.payload_type != payload_type {
                return Err(contract(format!(
                    "provenance.payload_type {payload_type:?} does not match the envelope payloadType {:?}",
                    envelope.payload_type
                )));
            }
            let key_file = resolve_under(base_dir, trusted_key_path, "trusted_key_path")?;
            let key_bytes = std::fs::read(&key_file).map_err(|e| {
                contract(format!(
                    "cannot read provenance.trusted_key_path {trusted_key_path:?}: {e}"
                ))
            })?;
            let trusted_key: TrustedKey = serde_json::from_slice(&key_bytes)
                .map_err(|e| contract(format!("malformed TrustedKey {trusted_key_path:?}: {e}")))?;
            store = TrustStore::from_pinned_roots(vec![trusted_key]).map_err(|e| {
                contract(format!("invalid pinned key in {trusted_key_path:?}: {e}"))
            })?;
            ProvenanceInput::Dsse(envelope)
        }
        // Keyless Sigstore is modeled but DEFERRED in this slice — rejected, never silently ignored.
        "sigstore_bundle" => {
            return Err(contract(
                "provenance kind \"sigstore_bundle\" is not yet wired into this emitter (supported: none | unsupported | dsse); a follow-up adds the keyless Sigstore path",
            ))
        }
        other => {
            return Err(contract(format!(
                "unknown provenance.kind {other:?} (expected none | unsupported | dsse | sigstore_bundle)"
            )))
        }
    };

    Ok(verify_supply_chain(VerifyInput {
        subject,
        expected_artifact_digest: d.expected_artifact_digest,
        provenance,
        pinning,
        policy,
        trust_store: &store,
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

    // dsse `envelope_path`/`trusted_key_path` resolve relative to the descriptor file's directory.
    let base_dir = Path::new(&args.input)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let carrier = match build_carrier(&raw, base_dir) {
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
    fn bc(raw: &str) -> Result<SupplyChainConformance, EmitErr> {
        // none/unsupported never consult base_dir; "." is a harmless placeholder for those cases.
        build_carrier(raw, Path::new("."))
    }
    fn carrier_value(raw: &str) -> Value {
        serde_json::to_value(bc(raw).expect("carrier")).expect("value")
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
        assert_eq!(bc(&d.to_string()).err().unwrap().code, EXIT_CONFIG_ERROR);
    }

    #[test]
    fn unknown_top_level_field_is_rejected() {
        let mut d: Value = serde_json::from_str(&descriptor(json!({ "kind": "none" }))).unwrap();
        d["trust_me_bro"] = json!(true);
        assert!(bc(&d.to_string()).is_err());
    }

    #[test]
    fn unknown_provenance_kind_is_rejected() {
        assert!(bc(&descriptor(json!({ "kind": "weird" }))).is_err());
    }

    #[test]
    fn unsupported_without_format_is_rejected() {
        assert!(bc(&descriptor(json!({ "kind": "unsupported" }))).is_err());
    }

    #[test]
    fn unknown_unsupported_format_is_rejected() {
        assert!(bc(&descriptor(
            json!({ "kind": "unsupported", "format": "made_up" })
        ))
        .is_err());
    }

    #[test]
    fn dsse_without_required_fields_is_rejected() {
        // dsse is now wired; omitting its required fields is a config error, never a silent None.
        let e = bc(&descriptor(json!({ "kind": "dsse" }))).err().unwrap();
        assert_eq!(e.code, EXIT_CONFIG_ERROR);
        assert!(e.msg.contains("is required for kind \"dsse\""));
    }

    #[test]
    fn deferred_sigstore_bundle_is_rejected_not_ignored() {
        assert!(bc(&descriptor(json!({ "kind": "sigstore_bundle" }))).is_err());
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
        assert!(bc(&d.to_string()).is_err());
    }

    // ---- DSSE pinned-key path -------------------------------------------------------------------
    // Fixtures are generated deterministically (fixed seed) in a tempdir per test; no committed crypto
    // blobs. The crypto/PAE/in-toto/policy live in `assay_registry` and are exercised through it here.
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    use ed25519_dalek::ed25519::signature::Signer;
    use ed25519_dalek::pkcs8::EncodePublicKey;
    use ed25519_dalek::SigningKey;
    use sha2::{Digest, Sha256};
    use std::fs;
    use tempfile::{tempdir, TempDir};

    const DSSE_ART_HEX: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const DSSE_ART: &str =
        "sha256:1111111111111111111111111111111111111111111111111111111111111111";
    const DSSE_BUILDER: &str = "https://github.com/example/builder@refs/tags/v1";

    /// DSSE PAE, byte-identical to the verifier's `build_pae` (the bytes the signature is computed over).
    fn pae(payload_type: &str, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"DSSEv1 ");
        v.extend_from_slice(payload_type.len().to_string().as_bytes());
        v.push(b' ');
        v.extend_from_slice(payload_type.as_bytes());
        v.push(b' ');
        v.extend_from_slice(payload.len().to_string().as_bytes());
        v.push(b' ');
        v.extend_from_slice(payload);
        v
    }
    fn key_id_of(spki_der: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(spki_der);
        let hex: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
        format!("sha256:{hex}")
    }
    /// A deterministic Ed25519 key from `seed`; returns (signer, key_id, TrustedKey JSON).
    fn fixture_key(seed: u8) -> (SigningKey, String, String) {
        let sk = SigningKey::from_bytes(&[seed; 32]);
        let der = sk
            .verifying_key()
            .to_public_key_der()
            .unwrap()
            .as_bytes()
            .to_vec();
        let key_id = key_id_of(&der);
        let trusted = json!({
            "key_id": key_id, "algorithm": "Ed25519", "public_key": B64.encode(&der)
        })
        .to_string();
        (sk, key_id, trusted)
    }
    fn statement(subject_hex: &str, predicate_type: &str, builder: &str) -> String {
        json!({
            "_type": "https://in-toto.io/Statement/v1",
            "subject": [{ "name": "art", "digest": { "sha256": subject_hex } }],
            "predicateType": predicate_type,
            "predicate": { "runDetails": { "builder": { "id": builder } } }
        })
        .to_string()
    }
    fn envelope(sk: &SigningKey, key_id: &str, statement: &str) -> String {
        let payload = statement.as_bytes();
        let sig = sk.sign(&pae(DSSE_IN_TOTO_PAYLOAD_TYPE, payload));
        json!({
            "payloadType": DSSE_IN_TOTO_PAYLOAD_TYPE,
            "payload": B64.encode(payload),
            "signatures": [{ "keyid": key_id, "sig": B64.encode(sig.to_bytes()) }]
        })
        .to_string()
    }
    /// Write envelope + key into a fresh tempdir and return (descriptor_raw, dir). The descriptor binds
    /// `expected_artifact_digest = DSSE_ART` and a policy requiring the matching builder at L1.
    fn dsse_case(
        envelope_json: &str,
        trusted_json: &str,
        extra_policy: Value,
    ) -> (String, TempDir) {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("env.dsse.json"), envelope_json).unwrap();
        fs::write(dir.path().join("key.trustedkey.json"), trusted_json).unwrap();
        let mut policy =
            json!({ "required_builder_id": DSSE_BUILDER, "required_slsa_build_level": 1 });
        if let Value::Object(extra) = extra_policy {
            for (k, val) in extra {
                policy[k] = val;
            }
        }
        let desc = json!({
            "schema": INPUT_SCHEMA,
            "subject": { "name": "art", "version": "1.0.0", "digest": DSSE_ART },
            "expected_artifact_digest": DSSE_ART,
            "provenance": {
                "kind": "dsse",
                "payload_type": DSSE_IN_TOTO_PAYLOAD_TYPE,
                "envelope_path": "env.dsse.json",
                "trusted_key_path": "key.trustedkey.json"
            },
            "pinning": { "version_pinned": true, "digest_pinned": true, "lockfile_digest": DSSE_ART },
            "policy": policy
        })
        .to_string();
        (desc, dir)
    }

    #[test]
    fn dsse_pinned_key_happy_path_emits_a_pass_carrier() {
        let (sk, key_id, trusted) = fixture_key(7);
        let stmt = statement(DSSE_ART_HEX, "https://slsa.dev/provenance/v1", DSSE_BUILDER);
        let (desc, dir) = dsse_case(&envelope(&sk, &key_id, &stmt), &trusted, json!({}));
        let v = serde_json::to_value(build_carrier(&desc, dir.path()).expect("carrier")).unwrap();
        assert_eq!(v["checks"]["provenance"]["dsse_signature"], "verified");
        assert_eq!(
            v["checks"]["integrity"]["subject_digest_binding"],
            "verified"
        );
        assert_eq!(v["policy_result"], "pass");
        // pinned-key DSSE is NOT the Sigstore leaf path, and never claims transparency-log evidence.
        assert_ne!(v["checks"]["provenance"]["dsse_pae"], "verified");
        assert_ne!(v["checks"]["provenance"]["rekor_inclusion"], "verified");
    }

    #[test]
    fn dsse_wrong_pinned_key_is_not_clean() {
        // Envelope signed by key A; trust store pins a DIFFERENT key B -> keyid A not found.
        let (sk, signed_key_id, _) = fixture_key(7);
        let (_, _, other_trusted) = fixture_key(9);
        let stmt = statement(DSSE_ART_HEX, "https://slsa.dev/provenance/v1", DSSE_BUILDER);
        let (desc, dir) = dsse_case(
            &envelope(&sk, &signed_key_id, &stmt),
            &other_trusted,
            json!({}),
        );
        let v = serde_json::to_value(build_carrier(&desc, dir.path()).expect("carrier")).unwrap();
        assert_ne!(v["policy_result"], "pass");
        assert_eq!(
            v["checks"]["provenance"]["dsse_signature"],
            "trust_root_unavailable"
        );
    }

    #[test]
    fn dsse_tampered_payload_fails_the_signature() {
        let (sk, key_id, trusted) = fixture_key(7);
        let stmt = statement(DSSE_ART_HEX, "https://slsa.dev/provenance/v1", DSSE_BUILDER);
        let mut env: Value = serde_json::from_str(&envelope(&sk, &key_id, &stmt)).unwrap();
        // Swap the payload AFTER signing: the signature no longer matches the PAE over the new payload.
        let tampered = statement(
            DSSE_ART_HEX,
            "https://slsa.dev/provenance/v1",
            "https://evil/builder",
        );
        env["payload"] = json!(B64.encode(tampered.as_bytes()));
        let (desc, dir) = dsse_case(&env.to_string(), &trusted, json!({}));
        let v = serde_json::to_value(build_carrier(&desc, dir.path()).expect("carrier")).unwrap();
        assert_ne!(v["policy_result"], "pass");
        assert_eq!(v["checks"]["provenance"]["dsse_signature"], "failed");
    }

    #[test]
    fn dsse_subject_digest_mismatch_is_reported() {
        let (sk, key_id, trusted) = fixture_key(7);
        let other_hex = "2222222222222222222222222222222222222222222222222222222222222222";
        let stmt = statement(other_hex, "https://slsa.dev/provenance/v1", DSSE_BUILDER);
        let (desc, dir) = dsse_case(&envelope(&sk, &key_id, &stmt), &trusted, json!({}));
        let v = serde_json::to_value(build_carrier(&desc, dir.path()).expect("carrier")).unwrap();
        assert_ne!(v["policy_result"], "pass");
        assert_eq!(
            v["checks"]["integrity"]["subject_digest_binding"],
            "subject_digest_mismatch"
        );
    }

    #[test]
    fn dsse_require_rekor_inclusion_is_incomplete_never_pass() {
        // A DSSE-only carrier cannot satisfy a Rekor requirement: incomplete, never a magic pass.
        let (sk, key_id, trusted) = fixture_key(7);
        let stmt = statement(DSSE_ART_HEX, "https://slsa.dev/provenance/v1", DSSE_BUILDER);
        let (desc, dir) = dsse_case(
            &envelope(&sk, &key_id, &stmt),
            &trusted,
            json!({ "require_rekor_inclusion": true }),
        );
        let v = serde_json::to_value(build_carrier(&desc, dir.path()).expect("carrier")).unwrap();
        assert_eq!(v["policy_result"], "incomplete");
    }

    #[test]
    fn dsse_payload_type_must_be_in_toto() {
        let (_sk, _id, trusted) = fixture_key(7);
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("env.dsse.json"), "{}").unwrap();
        fs::write(dir.path().join("key.trustedkey.json"), &trusted).unwrap();
        let desc = json!({
            "schema": INPUT_SCHEMA,
            "subject": { "name": "art", "version": "1.0.0", "digest": DSSE_ART },
            "provenance": { "kind": "dsse", "payload_type": "application/x-wrong",
                "envelope_path": "env.dsse.json", "trusted_key_path": "key.trustedkey.json" },
            "pinning": { "version_pinned": true },
            "policy": { "required_slsa_build_level": 0 }
        })
        .to_string();
        let e = build_carrier(&desc, dir.path()).err().unwrap();
        assert_eq!(e.code, EXIT_CONFIG_ERROR);
        assert!(e.msg.contains("payload_type must be"));
    }

    #[test]
    fn dsse_path_safety_rejects_absolute_traversal_and_url() {
        let (_sk, _id, trusted) = fixture_key(7);
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("key.trustedkey.json"), &trusted).unwrap();
        for bad in ["/etc/passwd", "../escape.json", "https://evil/env.json"] {
            let desc = json!({
                "schema": INPUT_SCHEMA,
                "subject": { "name": "art", "version": "1.0.0", "digest": DSSE_ART },
                "provenance": { "kind": "dsse", "payload_type": DSSE_IN_TOTO_PAYLOAD_TYPE,
                    "envelope_path": bad, "trusted_key_path": "key.trustedkey.json" },
                "pinning": { "version_pinned": true },
                "policy": { "required_slsa_build_level": 0 }
            })
            .to_string();
            assert_eq!(
                build_carrier(&desc, dir.path()).err().unwrap().code,
                EXIT_CONFIG_ERROR,
                "path {bad:?} must be rejected"
            );
        }
    }

    #[test]
    fn dsse_trustedkey_with_wrong_key_id_is_rejected() {
        // TrustedKey.key_id != sha256(SPKI-DER): prepare_pinned_key rejects -> config error (non-zero).
        let (sk, key_id, _) = fixture_key(7);
        let der = sk
            .verifying_key()
            .to_public_key_der()
            .unwrap()
            .as_bytes()
            .to_vec();
        let bad_trusted = json!({
            "key_id": "sha256:deadbeef", "algorithm": "Ed25519", "public_key": B64.encode(&der)
        })
        .to_string();
        let stmt = statement(DSSE_ART_HEX, "https://slsa.dev/provenance/v1", DSSE_BUILDER);
        let (desc, dir) = dsse_case(&envelope(&sk, &key_id, &stmt), &bad_trusted, json!({}));
        let e = build_carrier(&desc, dir.path()).err().unwrap();
        assert_eq!(e.code, EXIT_CONFIG_ERROR);
        assert!(e.msg.contains("invalid pinned key"));
    }

    #[test]
    fn dsse_fields_on_non_dsse_kind_are_rejected() {
        let mut d: Value = serde_json::from_str(&descriptor(json!({ "kind": "none" }))).unwrap();
        d["provenance"]["envelope_path"] = json!("env.json");
        assert!(bc(&d.to_string()).is_err());
    }

    fn committed_example_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/supply_chain_conformance_dsse")
    }

    #[test]
    fn regen_committed_dsse_example() {
        // Gated: `REGEN_DSSE_EXAMPLE=1 cargo test ... regen_committed_dsse_example` (re)writes the
        // committed example. Deterministic (fixed seed), so the committed bytes are reproducible.
        if std::env::var_os("REGEN_DSSE_EXAMPLE").is_none() {
            return;
        }
        let dir = committed_example_dir();
        fs::create_dir_all(&dir).unwrap();
        let (sk, key_id, trusted) = fixture_key(7);
        let stmt = statement(DSSE_ART_HEX, "https://slsa.dev/provenance/v1", DSSE_BUILDER);
        // Trailing newlines so the regen output is stable under the end-of-file-fixer pre-commit hook.
        fs::write(
            dir.join("slsa-provenance.statement.json"),
            format!("{stmt}\n"),
        )
        .unwrap();
        fs::write(
            dir.join("slsa-provenance.dsse.json"),
            format!("{}\n", envelope(&sk, &key_id, &stmt)),
        )
        .unwrap();
        fs::write(
            dir.join("pinned-ed25519.trustedkey.json"),
            format!("{trusted}\n"),
        )
        .unwrap();
        let desc = json!({
            "schema": INPUT_SCHEMA,
            "subject": { "name": "example-artifact", "version": "1.0.0", "digest": DSSE_ART },
            "expected_artifact_digest": DSSE_ART,
            "provenance": {
                "kind": "dsse",
                "payload_type": DSSE_IN_TOTO_PAYLOAD_TYPE,
                "envelope_path": "slsa-provenance.dsse.json",
                "trusted_key_path": "pinned-ed25519.trustedkey.json"
            },
            "pinning": { "version_pinned": true, "digest_pinned": true, "lockfile_digest": DSSE_ART },
            "policy": { "required_builder_id": DSSE_BUILDER, "required_slsa_build_level": 1 }
        });
        fs::write(
            dir.join("input.dsse.example.json"),
            serde_json::to_string_pretty(&desc).unwrap() + "\n",
        )
        .unwrap();
    }

    #[test]
    fn committed_dsse_example_emits_a_pass_carrier() {
        // Drift guard: the committed example must still produce a pass carrier with a verified pinned-key
        // signature. If this fails after a deliberate change, re-run `regen_committed_dsse_example`.
        let dir = committed_example_dir();
        let raw = fs::read_to_string(dir.join("input.dsse.example.json"))
            .expect("committed example descriptor");
        let v = serde_json::to_value(build_carrier(&raw, &dir).expect("carrier")).unwrap();
        assert_eq!(v["policy_result"], "pass");
        assert_eq!(v["checks"]["provenance"]["dsse_signature"], "verified");
        assert_eq!(
            v["checks"]["integrity"]["subject_digest_binding"],
            "verified"
        );
    }
}
