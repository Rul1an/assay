use std::path::{Path, PathBuf};

use assay_registry::supply_chain::{
    verify_supply_chain, ContainerRef, PinningInput, Policy, ProvenanceInput, SlsaLevel, Subject,
    SupplyChainConformance, UnsupportedProvenance, VerifyInput,
};
use assay_registry::trust::TrustStore;
use assay_registry::types::{DsseEnvelope, TrustedKey};
use serde::Deserialize;

use crate::exit_codes::EXIT_CONFIG_ERROR;

/// The CLI input descriptor schema. NOT a carrier, NOT a trust statement: a local mapping into
/// `VerifyInput`. Versioned so it can never become an implicit, unversioned format.
pub(super) const INPUT_SCHEMA: &str = "assay.supply_chain_conformance.input.v0";

/// The only DSSE payload type this slice accepts (an in-toto Statement). The CLI binds it; the
/// `assay_registry` verifier re-checks it (a mismatch there yields `dsse_signature: unsupported_format`).
pub(super) const DSSE_IN_TOTO_PAYLOAD_TYPE: &str = "application/vnd.in-toto+json";

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

/// `kind` in {none, unsupported, dsse, sigstore_bundle}. `format` is required iff kind==unsupported.
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
pub(super) struct EmitErr {
    pub(super) code: i32,
    pub(super) msg: String,
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

pub(super) fn build_carrier(raw: &str, base_dir: &Path) -> Result<SupplyChainConformance, EmitErr> {
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
            )));
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
                return Err(contract(
                    "provenance.format is only valid for kind \"unsupported\"",
                ));
            }
            reject_dsse_fields(&d.provenance, "none")?;
            store = TrustStore::new();
            ProvenanceInput::None
        }
        "unsupported" => {
            let format = d.provenance.format.as_deref().ok_or_else(|| {
                contract("provenance.format is required for kind \"unsupported\"")
            })?;
            let kind = match format {
                "pep740" => UnsupportedProvenance::Pep740,
                "npm_provenance" => UnsupportedProvenance::NpmProvenance,
                "unknown_predicate" => UnsupportedProvenance::UnknownPredicate,
                other => {
                    return Err(contract(format!(
                        "unknown provenance.format {other:?} (expected pep740 | npm_provenance | unknown_predicate)"
                    )));
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
                return Err(contract(
                    "provenance.format is only valid for kind \"unsupported\"",
                ));
            }
            let payload_type =
                d.provenance.payload_type.as_deref().ok_or_else(|| {
                    contract("provenance.payload_type is required for kind \"dsse\"")
                })?;
            let envelope_path = d.provenance.envelope_path.as_deref().ok_or_else(|| {
                contract("provenance.envelope_path is required for kind \"dsse\"")
            })?;
            let trusted_key_path = d.provenance.trusted_key_path.as_deref().ok_or_else(|| {
                contract("provenance.trusted_key_path is required for kind \"dsse\"")
            })?;
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
        // Keyless Sigstore is modeled but DEFERRED in this slice - rejected, never silently ignored.
        "sigstore_bundle" => {
            return Err(contract(
                "provenance kind \"sigstore_bundle\" is not yet wired into this emitter (supported: none | unsupported | dsse); a follow-up adds the keyless Sigstore path",
            ));
        }
        other => {
            return Err(contract(format!(
                "unknown provenance.kind {other:?} (expected none | unsupported | dsse | sigstore_bundle)"
            )));
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
