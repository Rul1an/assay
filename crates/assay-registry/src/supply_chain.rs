//! MCP04a-1 — supply-chain provenance/integrity/pinning conformance (coverage-honest).
//!
//! Produces the `assay.supply_chain_conformance.v0` carrier by verifying the supply-chain evidence
//! that is PRESENT on an MCP pack/server: artifact digest, a pinned-key in-toto/SLSA-over-DSSE build
//! provenance (subject-digest binding, builder identity, declared-vs-verified SLSA level), and
//! lockfile/digest pinning. Each check carries a precise status; `not_present` is a reported gap, never
//! a silent pass; a policy may require a minimum SLSA level. This is a VSA-style consumer-verifier
//! (trust-but-verify); it does NOT prove code safety, absence of malicious behaviour, or provider
//! trustworthiness beyond the verified attestation identity.
//!
//! Scope boundary (MCP04a-1): Assay's DSSE crypto is Ed25519-only, so this slice verifies pinned-key
//! in-toto/SLSA provenance. Sigstore keyless (Fulcio ECDSA + X.509 + Rekor) and the PEP 740 / npm
//! ecosystem adapters need a different crypto stack and are MCP04a-3; encountered here they are reported
//! `unsupported_format`, never passed. No new dependencies are introduced in this slice.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::{Signature, Verifier};
use serde::{Deserialize, Serialize};

use crate::dsse::verify_dsse_envelope_offline;
use crate::rekor::{verify_rekor_v2_inclusion_offline, TransparencyRequirement};
use crate::sigstore_identity::{verify_identity_offline, ExpectedIdentity};
use crate::sigstore_offline::verify_cert_chain_offline;
use crate::sigstore_signature::bind_in_toto_subject_digest;
use crate::trust::TrustStore;
use crate::types::DsseEnvelope;

pub const SCHEMA: &str = "assay.supply_chain_conformance.v0";
const STATEMENT_TYPE_V1: &str = "https://in-toto.io/Statement/v1";
const SLSA_PROVENANCE_PREDICATE: &str = "https://slsa.dev/provenance/v1";
const DSSE_PAYLOAD_TYPE: &str = "application/vnd.in-toto+json";

/// Per-check status. Append-only enum (do not reinterpret a value); each value is a distinct fact so
/// the consumer never has to guess semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Verified,
    Failed,
    NotPresent,
    NotApplicable,
    UnsupportedFormat,
    TrustRootUnavailable,
    OnlineRequired,
    PolicyNotSatisfied,
    SubjectDigestMismatch,
    IdentityMismatch,
    /// A dimension that is relevant but deliberately NOT verified in this slice (e.g. timestamp
    /// freshness / log consistency / witness cosignatures are not computed offline). Distinct from
    /// `NotApplicable` (does not apply) and `NotPresent` (applies but is absent). Append-only addition
    /// (MCP04a-3.4): a consumer that has not been updated must NOT read it as `verified` — see the paired
    /// Plimsoll a-3.4b consumer.
    NotChecked,
}

impl CheckStatus {
    /// A status that actively blocks (a verification that did not hold).
    fn is_blocking(self) -> bool {
        matches!(
            self,
            CheckStatus::Failed
                | CheckStatus::SubjectDigestMismatch
                | CheckStatus::IdentityMismatch
                | CheckStatus::PolicyNotSatisfied
        )
    }
    /// A status that is unresolved rather than failed (present-but-unverifiable / absent).
    fn is_pending(self) -> bool {
        matches!(
            self,
            CheckStatus::NotPresent
                | CheckStatus::UnsupportedFormat
                | CheckStatus::TrustRootUnavailable
                | CheckStatus::OnlineRequired
        )
    }
}

/// SLSA build level. `L0` = no provenance; `L1` = provenance exists + binds; `L2` = signed provenance
/// from an identified builder verified against the pinned trust root. `L3` (hardened build platform) is
/// NOT offline-provable in this slice, so a declared `L3` is reported `failed`, never passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SlsaLevel(pub u8);

impl SlsaLevel {
    pub fn label(self) -> String {
        format!("L{}", self.0)
    }
}

impl Serialize for SlsaLevel {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.label())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Subject {
    pub name: String,
    pub version: String,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntegrityChecks {
    pub artifact_digest: CheckStatus,
    pub subject_digest_binding: CheckStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProvenanceChecks {
    pub dsse_signature: CheckStatus,
    pub slsa_provenance: CheckStatus,
    pub builder_identity: CheckStatus,
    pub sigstore_bundle: CheckStatus,
    pub rekor_inclusion: CheckStatus,
    // --- MCP04a-3.4 append-only Sigstore-keyless dimensions ---
    /// Fulcio leaf chains to a pinned root (a-3.1). Distinct from `dsse_signature` (pinned-key Ed25519).
    pub cert_chain: CheckStatus,
    /// Fulcio SAN + OIDC issuer match the expected identity (a-3.2a). DISTINCT from `builder_identity`
    /// (the SLSA builder id) — this is the keyless signer identity.
    pub identity: CheckStatus,
    /// DSSE PAE signature verifies under the leaf key (a-3.3a). Distinct from `dsse_signature` (the
    /// pinned-key in-toto/SLSA path).
    pub dsse_pae: CheckStatus,
    /// Signing-time freshness (RFC3161 TSA). Not computed offline in this slice -> `NotChecked`.
    pub timestamp_freshness: CheckStatus,
    /// Transparency-log consistency proof. Not computed offline in this slice -> `NotChecked`.
    pub consistency: CheckStatus,
    /// Witness cosignatures on the checkpoint. Not computed offline in this slice -> `NotChecked`.
    pub witnessing: CheckStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct PinningChecks {
    pub version_pinned: CheckStatus,
    pub digest_pinned: CheckStatus,
    pub lockfile_subject_matches_artifact: CheckStatus,
    pub no_floating_source_ref: CheckStatus,
    pub no_tag_only_container_ref: CheckStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct Checks {
    pub integrity: IntegrityChecks,
    pub provenance: ProvenanceChecks,
    pub pinning: PinningChecks,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeclaredLevel {
    pub required_slsa_build_level: SlsaLevel,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifiedLevel {
    pub slsa_build_level: SlsaLevel,
}

#[derive(Debug, Clone, Serialize)]
pub struct Coverage {
    pub sources_checked: Vec<String>,
    pub limits: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyResult {
    Pass,
    Fail,
    Incomplete,
}

#[derive(Debug, Clone, Serialize)]
pub struct SupplyChainConformance {
    pub schema: String,
    pub subject: Subject,
    pub checks: Checks,
    pub declared: DeclaredLevel,
    pub verified: VerifiedLevel,
    pub policy_result: PolicyResult,
    pub coverage: Coverage,
    pub non_claims: Vec<String>,
}

// ---- Inputs -------------------------------------------------------------------------------------

/// Provenance encountered on the artifact. `Dsse` is the pinned-key in-toto/SLSA path; `SigstoreBundle`
/// is the MCP04a-3.4 keyless path (Fulcio + DSSE + Rekor v2), composed from the a-3.1..a-3.3c primitives;
/// `Unsupported` is recorded `unsupported_format`.
pub enum ProvenanceInput {
    None,
    Dsse(DsseEnvelope),
    SigstoreBundle(Box<SigstoreBundleInput>),
    Unsupported(UnsupportedProvenance),
}

#[derive(Debug, Clone, Copy)]
pub enum UnsupportedProvenance {
    Pep740,
    NpmProvenance,
    UnknownPredicate,
}

/// A keyless Sigstore DSSE bundle plus the PINNED trust material needed to verify it offline. The bundle
/// supplies only the leaf certificate + DSSE content; Fulcio roots/intermediates and the Rekor trusted
/// root are the verifier's own pinned material (never taken from the bundle).
pub struct SigstoreBundleInput {
    pub bundle_json: Vec<u8>,
    pub fulcio_roots: Vec<Vec<u8>>,
    pub fulcio_intermediates: Vec<Vec<u8>>,
    pub rekor_trusted_root_json: Vec<u8>,
    /// Verification time for the cert-chain validity window (cert validity, NOT timestamp freshness).
    pub now_unix_secs: u64,
    pub expected_san: String,
    pub expected_issuer: String,
    /// Whether transparency-log inclusion is required (drives the Rekor verifier's missing-proof status).
    pub rekor_requirement: TransparencyRequirement,
}

/// A Sigstore bundle parsed exactly ONCE into neutral evidence (a-3.4 design-of-record): every Sigstore
/// dimension is computed from the SAME bytes, so `identity` can never read one leaf while `rekor` binds
/// another. The raw bundle bytes go to the Rekor verifier directly (it parses its own tlog section).
struct ParsedSigstoreBundleEvidence {
    leaf_der: Vec<u8>,
    dsse_envelope_bytes: Vec<u8>,
    statement_payload: Vec<u8>,
}

const BUNDLE_MEDIA_TYPE_V0_3: &str = "application/vnd.dev.sigstore.bundle.v0.3+json";

/// Shape/availability gate for the Sigstore DSSE path. Returns the neutral evidence on success, or the
/// `sigstore_bundle` `CheckStatus` to record on failure: `UnsupportedFormat` for a well-formed but
/// unsupported shape (wrong mediaType, `messageSignature`, `x509CertificateChain`/`publicKey` material,
/// missing certificate), `Failed` for bytes that should have parsed but did not (malformed JSON, missing
/// content, un-decodable leaf). Mirrors the a-3.3b shape gate, but a-3.4 owns the parse so the dimensions
/// share one evidence object. NOT a trust verdict — the trust verdicts are the orthogonal dimensions.
fn parse_sigstore_bundle(bundle_json: &[u8]) -> Result<ParsedSigstoreBundleEvidence, CheckStatus> {
    let bundle: serde_json::Value =
        serde_json::from_slice(bundle_json).map_err(|_| CheckStatus::Failed)?;

    if bundle.get("mediaType").and_then(|v| v.as_str()) != Some(BUNDLE_MEDIA_TYPE_V0_3) {
        return Err(CheckStatus::UnsupportedFormat);
    }
    if bundle.get("messageSignature").is_some() {
        return Err(CheckStatus::UnsupportedFormat);
    }
    let dsse = match bundle.get("dsseEnvelope") {
        Some(v) => v,
        None => return Err(CheckStatus::Failed), // no content
    };
    let material = match bundle.get("verificationMaterial") {
        Some(v) => v,
        None => return Err(CheckStatus::Failed),
    };
    if material.get("x509CertificateChain").is_some() || material.get("publicKey").is_some() {
        return Err(CheckStatus::UnsupportedFormat);
    }
    let raw_bytes = match material
        .pointer("/certificate/rawBytes")
        .and_then(|v| v.as_str())
    {
        Some(s) => s,
        None => return Err(CheckStatus::UnsupportedFormat),
    };
    let leaf_der = BASE64
        .decode(raw_bytes.as_bytes())
        .map_err(|_| CheckStatus::Failed)?;
    let dsse_envelope_bytes = serde_json::to_vec(dsse).map_err(|_| CheckStatus::Failed)?;
    // Best-effort: the in-toto statement payload for subject binding. A missing/undecodable payload is not
    // a shape failure here (the dsse_pae / subject dimensions report it); bind over empty -> Failed.
    let statement_payload = dsse
        .get("payload")
        .and_then(|v| v.as_str())
        .and_then(|p| BASE64.decode(p.as_bytes()).ok())
        .unwrap_or_default();

    Ok(ParsedSigstoreBundleEvidence {
        leaf_der,
        dsse_envelope_bytes,
        statement_payload,
    })
}

#[derive(Debug, Clone, Copy)]
pub enum ContainerRef {
    DigestPinned,
    TagOnly,
}

pub struct PinningInput {
    pub version_pinned: bool,
    pub digest_pinned: Option<bool>,
    /// Digest recorded in the lockfile for this subject, if any (compared to the artifact digest).
    pub lockfile_digest: Option<String>,
    pub floating_source_ref: bool,
    pub container_ref: Option<ContainerRef>,
}

pub struct Policy {
    pub required_builder_id: Option<String>,
    pub required_slsa_build_level: SlsaLevel,
    // --- MCP04a-3.4 transparency-extension requirements (offline-first: false = report-only) ---
    /// Require transparency-log inclusion: a non-`Verified` `rekor_inclusion` is then Incomplete.
    pub require_rekor_inclusion: bool,
    /// Require signing-time freshness: `timestamp_freshness` is `NotChecked` offline, so requiring it
    /// yields Incomplete (never a magic pass).
    pub require_timestamp_freshness: bool,
    /// Require a log-consistency proof: `consistency` is `NotChecked` offline -> Incomplete when required.
    pub require_consistency: bool,
    /// Require witness cosignatures: `witnessing` is `NotChecked` offline -> Incomplete when required.
    pub require_witnessing: bool,
}

pub struct VerifyInput<'a> {
    pub subject: Subject,
    /// Optional expected artifact digest (e.g. from a manifest); compared to the computed subject digest.
    pub expected_artifact_digest: Option<String>,
    pub provenance: ProvenanceInput,
    pub pinning: PinningInput,
    pub policy: Policy,
    pub trust_store: &'a TrustStore,
}

// ---- in-toto / SLSA parsing (serde_json, no new dep) --------------------------------------------

#[derive(Deserialize)]
struct InTotoStatement {
    #[serde(rename = "_type")]
    type_: String,
    subject: Vec<InTotoSubject>,
    #[serde(rename = "predicateType")]
    predicate_type: String,
    #[serde(default)]
    predicate: serde_json::Value,
}

#[derive(Deserialize)]
struct InTotoSubject {
    #[serde(default)]
    digest: std::collections::BTreeMap<String, String>,
}

fn hex_of(d: &str) -> &str {
    d.strip_prefix("sha256:").unwrap_or(d)
}

// ---- Verification --------------------------------------------------------------------------------

fn build_pae(payload_type: &str, payload: &[u8]) -> Vec<u8> {
    let mut pae = Vec::new();
    pae.extend_from_slice(b"DSSEv1 ");
    pae.extend_from_slice(payload_type.len().to_string().as_bytes());
    pae.push(b' ');
    pae.extend_from_slice(payload_type.as_bytes());
    pae.push(b' ');
    pae.extend_from_slice(payload.len().to_string().as_bytes());
    pae.push(b' ');
    pae.extend_from_slice(payload);
    pae
}

/// Verify a DSSE envelope's signatures against the pinned trust store. Distinguishes "no trusted key
/// for this key id" (`TrustRootUnavailable`) from "key found but signature invalid" (`Failed`).
fn verify_dsse_signature(envelope: &DsseEnvelope, trust_store: &TrustStore) -> CheckStatus {
    if envelope.payload_type != DSSE_PAYLOAD_TYPE {
        return CheckStatus::UnsupportedFormat;
    }
    let payload_bytes = match BASE64.decode(&envelope.payload) {
        Ok(b) => b,
        Err(_) => return CheckStatus::Failed,
    };
    if envelope.signatures.is_empty() {
        return CheckStatus::NotPresent;
    }
    let pae = build_pae(&envelope.payload_type, &payload_bytes);
    let mut any_key_found = false;
    for sig in &envelope.signatures {
        let key = match trust_store.get_key(&sig.key_id) {
            Ok(k) => k,
            Err(_) => continue, // key not trusted: try the next signature
        };
        any_key_found = true;
        let sig_bytes = match BASE64.decode(&sig.signature) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let signature = match Signature::from_slice(&sig_bytes) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if key.verify(&pae, &signature).is_ok() {
            return CheckStatus::Verified;
        }
    }
    // No signature verified: distinguish "no trusted key at all" from "key found but invalid".
    if any_key_found {
        CheckStatus::Failed
    } else {
        CheckStatus::TrustRootUnavailable
    }
}

fn decode_statement(envelope: &DsseEnvelope) -> Option<InTotoStatement> {
    let payload = BASE64.decode(&envelope.payload).ok()?;
    serde_json::from_slice::<InTotoStatement>(&payload).ok()
}

/// The whole provenance check group + the verified SLSA level.
struct ProvenanceOutcome {
    checks: ProvenanceChecks,
    subject_digest_binding: CheckStatus,
    verified_level: SlsaLevel,
}

fn verify_provenance(input: &VerifyInput<'_>) -> ProvenanceOutcome {
    let na = CheckStatus::NotApplicable;
    match &input.provenance {
        ProvenanceInput::None => ProvenanceOutcome {
            checks: ProvenanceChecks {
                dsse_signature: CheckStatus::NotPresent,
                slsa_provenance: CheckStatus::NotPresent,
                builder_identity: CheckStatus::NotPresent,
                sigstore_bundle: CheckStatus::NotPresent,
                rekor_inclusion: CheckStatus::NotPresent,
                cert_chain: CheckStatus::NotPresent,
                identity: CheckStatus::NotPresent,
                dsse_pae: CheckStatus::NotPresent,
                timestamp_freshness: na,
                consistency: na,
                witnessing: na,
            },
            subject_digest_binding: CheckStatus::NotPresent,
            verified_level: SlsaLevel(0),
        },
        ProvenanceInput::SigstoreBundle(sb) => {
            verify_sigstore_bundle_provenance(sb, &input.subject)
        }
        ProvenanceInput::Unsupported(kind) => {
            // PEP740 / npm adapters are not yet supported; report unsupported, never pass.
            let sigstore = match kind {
                UnsupportedProvenance::Pep740 | UnsupportedProvenance::NpmProvenance => {
                    CheckStatus::UnsupportedFormat
                }
                UnsupportedProvenance::UnknownPredicate => CheckStatus::NotApplicable,
            };
            ProvenanceOutcome {
                checks: ProvenanceChecks {
                    dsse_signature: CheckStatus::UnsupportedFormat,
                    slsa_provenance: CheckStatus::UnsupportedFormat,
                    builder_identity: CheckStatus::UnsupportedFormat,
                    sigstore_bundle: sigstore,
                    rekor_inclusion: CheckStatus::NotApplicable,
                    cert_chain: CheckStatus::UnsupportedFormat,
                    identity: CheckStatus::UnsupportedFormat,
                    dsse_pae: CheckStatus::UnsupportedFormat,
                    timestamp_freshness: na,
                    consistency: na,
                    witnessing: na,
                },
                subject_digest_binding: CheckStatus::NotApplicable,
                verified_level: SlsaLevel(0),
            }
        }
        ProvenanceInput::Dsse(env) => {
            let dsse_signature = verify_dsse_signature(env, input.trust_store);
            let statement = decode_statement(env);

            // Subject-digest binding: the provenance subject digest must equal the artifact digest.
            let want = hex_of(&input.subject.digest);
            let subject_digest_binding = match &statement {
                Some(s) if s.type_ == STATEMENT_TYPE_V1 => {
                    let bound = s
                        .subject
                        .iter()
                        .filter_map(|sub| sub.digest.get("sha256"))
                        .any(|d| hex_of(d) == want);
                    if bound {
                        CheckStatus::Verified
                    } else {
                        CheckStatus::SubjectDigestMismatch
                    }
                }
                _ => CheckStatus::Failed,
            };

            // Predicate must be SLSA provenance; otherwise the format is unsupported.
            let is_slsa = statement
                .as_ref()
                .map(|s| s.predicate_type == SLSA_PROVENANCE_PREDICATE)
                .unwrap_or(false);

            // Builder identity from the SLSA predicate (runDetails.builder.id).
            let builder_id = statement
                .as_ref()
                .and_then(|s| s.predicate.pointer("/runDetails/builder/id"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let builder_identity = match (&input.policy.required_builder_id, &builder_id) {
                _ if !is_slsa => CheckStatus::UnsupportedFormat,
                (Some(req), Some(got)) if req == got => CheckStatus::Verified,
                (Some(_), Some(_)) => CheckStatus::IdentityMismatch,
                (Some(_), None) => CheckStatus::IdentityMismatch,
                (None, Some(_)) => CheckStatus::Verified,
                (None, None) => na,
            };

            // Verified level: L1 = parsed + binds; L2 = + signature verified + builder identity ok.
            // L3 (hardened platform) is never assertable offline in this slice.
            let binds = subject_digest_binding == CheckStatus::Verified;
            let signed_ok = dsse_signature == CheckStatus::Verified;
            let identity_ok = matches!(
                builder_identity,
                CheckStatus::Verified | CheckStatus::NotApplicable
            );
            let verified_level = if !is_slsa || !binds {
                SlsaLevel(0)
            } else if signed_ok && identity_ok {
                SlsaLevel(2)
            } else {
                SlsaLevel(1)
            };

            // SLSA provenance check: unsupported predicate, else declared-vs-verified.
            let required = input.policy.required_slsa_build_level;
            let slsa_provenance = if !is_slsa {
                CheckStatus::UnsupportedFormat
            } else if verified_level >= required {
                CheckStatus::Verified
            } else {
                // Includes "declared L3 but unverifiable -> failed".
                CheckStatus::Failed
            };

            ProvenanceOutcome {
                checks: ProvenanceChecks {
                    dsse_signature,
                    slsa_provenance,
                    builder_identity,
                    sigstore_bundle: CheckStatus::NotApplicable,
                    rekor_inclusion: CheckStatus::NotApplicable,
                    // The keyless Sigstore dimensions do not apply to the pinned-key DSSE path.
                    cert_chain: na,
                    identity: na,
                    dsse_pae: na,
                    timestamp_freshness: na,
                    consistency: na,
                    witnessing: na,
                },
                subject_digest_binding,
                verified_level,
            }
        }
    }
}

/// Compose the offline Sigstore-keyless primitives (a-3.1 chain, a-3.2a identity, a-3.3a DSSE/PAE,
/// a-3.2b subject binding, a-3.3c Rekor v2 inclusion) into the provenance dimensions, computed ORTHOGONALLY
/// from a single shared parse of the bundle. A failing dimension never short-circuits the others; the only
/// early-exit is a bundle shape/parse failure (then the dependent dimensions inherit that status). DSSE/PAE
/// and Rekor inclusion are cryptographically coupled by the Rekor v2 leaf-bind; identity is the independent
/// policy axis Rekor does not bind. `sigstore_bundle=Verified` means the shape was decomposable, NOT a
/// trust verdict.
fn verify_sigstore_bundle_provenance(
    sb: &SigstoreBundleInput,
    subject: &Subject,
) -> ProvenanceOutcome {
    let na = CheckStatus::NotApplicable;
    let want = hex_of(&subject.digest);
    match parse_sigstore_bundle(&sb.bundle_json) {
        Err(status) => ProvenanceOutcome {
            checks: ProvenanceChecks {
                dsse_signature: na,
                slsa_provenance: na,
                builder_identity: na,
                sigstore_bundle: status,
                rekor_inclusion: status,
                cert_chain: status,
                identity: status,
                dsse_pae: status,
                timestamp_freshness: CheckStatus::NotChecked,
                consistency: CheckStatus::NotChecked,
                witnessing: CheckStatus::NotChecked,
            },
            subject_digest_binding: status,
            verified_level: SlsaLevel(0),
        },
        Ok(ev) => {
            let roots: Vec<&[u8]> = sb.fulcio_roots.iter().map(|v| v.as_slice()).collect();
            let inters: Vec<&[u8]> = sb
                .fulcio_intermediates
                .iter()
                .map(|v| v.as_slice())
                .collect();
            let expected = ExpectedIdentity {
                san: &sb.expected_san,
                issuer: &sb.expected_issuer,
            };

            let cert_chain =
                verify_cert_chain_offline(&ev.leaf_der, &inters, &roots, sb.now_unix_secs).status;
            let identity =
                verify_identity_offline(&ev.leaf_der, &inters, &roots, sb.now_unix_secs, &expected)
                    .status;
            let dsse_pae =
                verify_dsse_envelope_offline(&ev.leaf_der, &ev.dsse_envelope_bytes, want).status;
            let subject_digest_binding =
                bind_in_toto_subject_digest(&ev.statement_payload, want).status;
            let rekor_inclusion = verify_rekor_v2_inclusion_offline(
                &sb.bundle_json,
                &sb.rekor_trusted_root_json,
                sb.rekor_requirement,
            )
            .status;

            ProvenanceOutcome {
                checks: ProvenanceChecks {
                    // The pinned-key in-toto/SLSA fields do not apply to the keyless path.
                    dsse_signature: na,
                    slsa_provenance: na,
                    builder_identity: na,
                    sigstore_bundle: CheckStatus::Verified,
                    rekor_inclusion,
                    cert_chain,
                    identity,
                    dsse_pae,
                    timestamp_freshness: CheckStatus::NotChecked,
                    consistency: CheckStatus::NotChecked,
                    witnessing: CheckStatus::NotChecked,
                },
                subject_digest_binding,
                verified_level: SlsaLevel(0),
            }
        }
    }
}

fn verify_pinning(p: &PinningInput, subject_digest: &str) -> PinningChecks {
    let version_pinned = if p.version_pinned {
        CheckStatus::Verified
    } else {
        CheckStatus::PolicyNotSatisfied
    };
    let digest_pinned = match p.digest_pinned {
        Some(true) => CheckStatus::Verified,
        Some(false) => CheckStatus::PolicyNotSatisfied,
        None => CheckStatus::NotApplicable,
    };
    let lockfile_subject_matches_artifact = match &p.lockfile_digest {
        Some(locked) if hex_of(locked) == hex_of(subject_digest) => CheckStatus::Verified,
        Some(_) => CheckStatus::Failed, // lockfile digest mismatch
        None => CheckStatus::NotPresent,
    };
    let no_floating_source_ref = if p.floating_source_ref {
        CheckStatus::PolicyNotSatisfied
    } else {
        CheckStatus::Verified
    };
    let no_tag_only_container_ref = match p.container_ref {
        Some(ContainerRef::DigestPinned) => CheckStatus::Verified,
        Some(ContainerRef::TagOnly) => CheckStatus::PolicyNotSatisfied,
        None => CheckStatus::NotApplicable,
    };
    PinningChecks {
        version_pinned,
        digest_pinned,
        lockfile_subject_matches_artifact,
        no_floating_source_ref,
        no_tag_only_container_ref,
    }
}

fn all_statuses(c: &Checks) -> Vec<CheckStatus> {
    vec![
        c.integrity.artifact_digest,
        c.integrity.subject_digest_binding,
        c.provenance.dsse_signature,
        c.provenance.slsa_provenance,
        c.provenance.builder_identity,
        c.provenance.sigstore_bundle,
        c.provenance.rekor_inclusion,
        c.provenance.cert_chain,
        c.provenance.identity,
        c.provenance.dsse_pae,
        c.provenance.timestamp_freshness,
        c.provenance.consistency,
        c.provenance.witnessing,
        c.pinning.version_pinned,
        c.pinning.digest_pinned,
        c.pinning.lockfile_subject_matches_artifact,
        c.pinning.no_floating_source_ref,
        c.pinning.no_tag_only_container_ref,
    ]
}

/// All statuses EXCEPT the transparency-extension dimensions (rekor_inclusion + timestamp/consistency/
/// witnessing). Those are optional-by-default (offline-first) and gated by `require_*`, so they are not
/// swept by the generic "any pending -> Incomplete" rule.
fn non_transparency_statuses(c: &Checks) -> Vec<CheckStatus> {
    vec![
        c.integrity.artifact_digest,
        c.integrity.subject_digest_binding,
        c.provenance.dsse_signature,
        c.provenance.slsa_provenance,
        c.provenance.builder_identity,
        c.provenance.sigstore_bundle,
        c.provenance.cert_chain,
        c.provenance.identity,
        c.provenance.dsse_pae,
        c.pinning.version_pinned,
        c.pinning.digest_pinned,
        c.pinning.lockfile_subject_matches_artifact,
        c.pinning.no_floating_source_ref,
        c.pinning.no_tag_only_container_ref,
    ]
}

/// Producer-side policy summary. Plimsoll (MCP04a-2 / a-3.4b) applies the nuanced, policy-aware mapping;
/// this is the carrier's own coarse verdict: any blocking status -> Fail; else a required transparency
/// dimension that is not verified, a required-but-unverified SLSA provenance, or any non-transparency
/// pending status -> Incomplete; else Pass. Transparency dimensions are offline-first: not-verified only
/// uncleans when the policy requires it (a `NotChecked` timestamp/consistency/witness is otherwise a
/// coverage limit, never a magic pass).
fn compute_policy_result(checks: &Checks, policy: &Policy) -> PolicyResult {
    if all_statuses(checks).iter().any(|s| s.is_blocking()) {
        return PolicyResult::Fail;
    }
    let p = &checks.provenance;
    let required_unverified = (policy.require_rekor_inclusion
        && p.rekor_inclusion != CheckStatus::Verified)
        || (policy.require_timestamp_freshness && p.timestamp_freshness != CheckStatus::Verified)
        || (policy.require_consistency && p.consistency != CheckStatus::Verified)
        || (policy.require_witnessing && p.witnessing != CheckStatus::Verified);
    if required_unverified {
        return PolicyResult::Incomplete;
    }
    let provenance_required = policy.required_slsa_build_level > SlsaLevel(0);
    if provenance_required && p.slsa_provenance != CheckStatus::Verified {
        return PolicyResult::Incomplete;
    }
    if non_transparency_statuses(checks)
        .iter()
        .any(|s| s.is_pending())
    {
        return PolicyResult::Incomplete;
    }
    PolicyResult::Pass
}

/// Verify the supply-chain evidence present on a subject and produce the conformance carrier.
pub fn verify_supply_chain(input: VerifyInput<'_>) -> SupplyChainConformance {
    let artifact_digest = match &input.expected_artifact_digest {
        Some(expected) if hex_of(expected) != hex_of(&input.subject.digest) => CheckStatus::Failed,
        _ => CheckStatus::Verified,
    };

    let prov = verify_provenance(&input);
    let pinning = verify_pinning(&input.pinning, &input.subject.digest);

    let checks = Checks {
        integrity: IntegrityChecks {
            artifact_digest,
            subject_digest_binding: prov.subject_digest_binding,
        },
        provenance: prov.checks,
        pinning,
    };
    let policy_result = compute_policy_result(&checks, &input.policy);

    // The keyless Sigstore path is in play exactly when the transparency extensions are NotChecked; only
    // then do we record the freshness/consistency/witness coverage gaps (honest, not over-claimed).
    let sigstore_path = checks.provenance.timestamp_freshness == CheckStatus::NotChecked;
    let mut limits = vec![
        "transitive dependencies not re-fetched".to_string(),
        "PEP 740 / npm provenance adapters not verified in this slice".to_string(),
        "live transparency-log lookup not performed offline".to_string(),
    ];
    if sigstore_path {
        limits
            .push("timestamp freshness not checked (RFC3161; Rekor v2 issues no SET)".to_string());
        limits.push("transparency-log consistency proof not checked".to_string());
        limits.push("witness cosignatures not checked".to_string());
    }

    SupplyChainConformance {
        schema: SCHEMA.to_string(),
        subject: input.subject,
        checks,
        declared: DeclaredLevel {
            required_slsa_build_level: input.policy.required_slsa_build_level,
        },
        verified: VerifiedLevel {
            slsa_build_level: prov.verified_level,
        },
        policy_result,
        coverage: Coverage {
            sources_checked: vec![
                "pack".to_string(),
                "lockfile".to_string(),
                "provenance".to_string(),
            ],
            limits,
        },
        non_claims: vec![
            "provenance verification does not prove code safety".to_string(),
            "verified provenance does not prove absence of malicious behaviour".to_string(),
            "verified signer identity is not a judgement that the provider is trustworthy"
                .to_string(),
            "not_present is not a silent pass when policy requires provenance".to_string(),
        ],
    }
}

/// A report is clean only when every applicable check is `verified` (`not_applicable` allowed). Every
/// other status is not-clean; the consumer decides whether a given not-clean status blocks or warns.
pub fn is_clean(report: &SupplyChainConformance) -> bool {
    report.schema == SCHEMA
        && all_statuses(&report.checks)
            .iter()
            .all(|s| matches!(s, CheckStatus::Verified | CheckStatus::NotApplicable))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DsseSignature, TrustedKey};
    use ed25519_dalek::ed25519::signature::Signer;
    use ed25519_dalek::{SigningKey, VerifyingKey};
    use sha2::{Digest, Sha256};

    const ARTIFACT_DIGEST: &str =
        "sha256:1111111111111111111111111111111111111111111111111111111111111111";
    const BUILDER: &str = "https://github.com/example/builder@refs/tags/v1";

    fn sha256_hex(bytes: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(bytes);
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    }

    fn spki_der(vk: &VerifyingKey) -> Vec<u8> {
        use ed25519_dalek::pkcs8::EncodePublicKey;
        vk.to_public_key_der().unwrap().as_bytes().to_vec()
    }

    /// Build a trust store with the given verifying key pinned; returns (store, key_id).
    fn trust_with(vk: &VerifyingKey) -> (TrustStore, String) {
        let der = spki_der(vk);
        let key_id = format!("sha256:{}", sha256_hex(&der));
        let key = TrustedKey {
            key_id: key_id.clone(),
            algorithm: "Ed25519".to_string(),
            public_key: BASE64.encode(&der),
            description: None,
            added_at: None,
            expires_at: None,
            revoked: false,
        };
        (TrustStore::from_pinned_roots(vec![key]).unwrap(), key_id)
    }

    fn statement_json(subject_digest_hex: &str, predicate_type: &str, builder: &str) -> String {
        serde_json::json!({
            "_type": STATEMENT_TYPE_V1,
            "subject": [{ "name": "pack", "digest": { "sha256": subject_digest_hex } }],
            "predicateType": predicate_type,
            "predicate": { "runDetails": { "builder": { "id": builder } } }
        })
        .to_string()
    }

    /// A signed in-toto/SLSA DSSE envelope over `statement`, signed by `sk`.
    fn signed_dsse(sk: &SigningKey, key_id: &str, statement: &str) -> DsseEnvelope {
        let payload_bytes = statement.as_bytes().to_vec();
        let payload_b64 = BASE64.encode(&payload_bytes);
        let pae = build_pae(DSSE_PAYLOAD_TYPE, &payload_bytes);
        let sig = sk.sign(&pae);
        DsseEnvelope {
            payload_type: DSSE_PAYLOAD_TYPE.to_string(),
            payload: payload_b64,
            signatures: vec![DsseSignature {
                key_id: key_id.to_string(),
                signature: BASE64.encode(sig.to_bytes()),
            }],
        }
    }

    fn subject() -> Subject {
        Subject {
            name: "mcp-pack".to_string(),
            version: "1.2.3".to_string(),
            digest: ARTIFACT_DIGEST.to_string(),
        }
    }

    fn clean_pinning() -> PinningInput {
        PinningInput {
            version_pinned: true,
            digest_pinned: Some(true),
            lockfile_digest: Some(ARTIFACT_DIGEST.to_string()),
            floating_source_ref: false,
            container_ref: Some(ContainerRef::DigestPinned),
        }
    }

    fn policy(level: u8) -> Policy {
        Policy {
            required_builder_id: Some(BUILDER.to_string()),
            required_slsa_build_level: SlsaLevel(level),
            require_rekor_inclusion: false,
            require_timestamp_freshness: false,
            require_consistency: false,
            require_witnessing: false,
        }
    }

    #[test]
    fn valid_pinned_key_slsa_provenance_is_verified_and_clean() {
        let sk = SigningKey::from_bytes(&[7u8; 32]);
        let (store, key_id) = trust_with(&sk.verifying_key());
        let env = signed_dsse(
            &sk,
            &key_id,
            &statement_json(hex_of(ARTIFACT_DIGEST), SLSA_PROVENANCE_PREDICATE, BUILDER),
        );
        let report = verify_supply_chain(VerifyInput {
            subject: subject(),
            expected_artifact_digest: Some(ARTIFACT_DIGEST.to_string()),
            provenance: ProvenanceInput::Dsse(env),
            pinning: clean_pinning(),
            policy: policy(2),
            trust_store: &store,
        });
        assert_eq!(
            report.checks.provenance.dsse_signature,
            CheckStatus::Verified
        );
        assert_eq!(
            report.checks.provenance.slsa_provenance,
            CheckStatus::Verified
        );
        assert_eq!(
            report.checks.provenance.builder_identity,
            CheckStatus::Verified
        );
        assert_eq!(
            report.checks.integrity.subject_digest_binding,
            CheckStatus::Verified
        );
        assert_eq!(report.verified.slsa_build_level, SlsaLevel(2));
        assert_eq!(report.policy_result, PolicyResult::Pass);
        assert!(is_clean(&report));
    }

    #[test]
    fn missing_provenance_is_not_present_never_clean() {
        let store = TrustStore::new();
        let report = verify_supply_chain(VerifyInput {
            subject: subject(),
            expected_artifact_digest: None,
            provenance: ProvenanceInput::None,
            pinning: clean_pinning(),
            policy: policy(2),
            trust_store: &store,
        });
        assert_eq!(
            report.checks.provenance.slsa_provenance,
            CheckStatus::NotPresent
        );
        assert_eq!(report.policy_result, PolicyResult::Incomplete);
        assert!(!is_clean(&report));
    }

    #[test]
    fn pep740_npm_are_unsupported_format_never_pass() {
        let store = TrustStore::new();
        for kind in [
            UnsupportedProvenance::Pep740,
            UnsupportedProvenance::NpmProvenance,
        ] {
            let report = verify_supply_chain(VerifyInput {
                subject: subject(),
                expected_artifact_digest: None,
                provenance: ProvenanceInput::Unsupported(kind),
                pinning: clean_pinning(),
                policy: policy(2),
                trust_store: &store,
            });
            assert_eq!(
                report.checks.provenance.slsa_provenance,
                CheckStatus::UnsupportedFormat
            );
            assert_eq!(
                report.checks.provenance.sigstore_bundle,
                CheckStatus::UnsupportedFormat
            );
            assert!(!is_clean(&report));
            assert_eq!(report.policy_result, PolicyResult::Incomplete);
        }
    }

    #[test]
    fn subject_digest_mismatch_fails() {
        let sk = SigningKey::from_bytes(&[9u8; 32]);
        let (store, key_id) = trust_with(&sk.verifying_key());
        let env = signed_dsse(
            &sk,
            &key_id,
            &statement_json("deadbeef", SLSA_PROVENANCE_PREDICATE, BUILDER), // wrong subject
        );
        let report = verify_supply_chain(VerifyInput {
            subject: subject(),
            expected_artifact_digest: None,
            provenance: ProvenanceInput::Dsse(env),
            pinning: clean_pinning(),
            policy: policy(1),
            trust_store: &store,
        });
        assert_eq!(
            report.checks.integrity.subject_digest_binding,
            CheckStatus::SubjectDigestMismatch
        );
        assert_eq!(report.policy_result, PolicyResult::Fail);
    }

    #[test]
    fn declared_l3_but_unverifiable_fails() {
        let sk = SigningKey::from_bytes(&[3u8; 32]);
        let (store, key_id) = trust_with(&sk.verifying_key());
        let env = signed_dsse(
            &sk,
            &key_id,
            &statement_json(hex_of(ARTIFACT_DIGEST), SLSA_PROVENANCE_PREDICATE, BUILDER),
        );
        let report = verify_supply_chain(VerifyInput {
            subject: subject(),
            expected_artifact_digest: None,
            provenance: ProvenanceInput::Dsse(env),
            pinning: clean_pinning(),
            policy: policy(3), // require L3; offline pinned-key can prove at most L2
            trust_store: &store,
        });
        assert_eq!(report.verified.slsa_build_level, SlsaLevel(2));
        assert_eq!(
            report.checks.provenance.slsa_provenance,
            CheckStatus::Failed
        );
        assert_eq!(report.policy_result, PolicyResult::Fail);
    }

    #[test]
    fn builder_identity_mismatch() {
        let sk = SigningKey::from_bytes(&[5u8; 32]);
        let (store, key_id) = trust_with(&sk.verifying_key());
        let env = signed_dsse(
            &sk,
            &key_id,
            &statement_json(
                hex_of(ARTIFACT_DIGEST),
                SLSA_PROVENANCE_PREDICATE,
                "https://evil/builder",
            ),
        );
        let report = verify_supply_chain(VerifyInput {
            subject: subject(),
            expected_artifact_digest: None,
            provenance: ProvenanceInput::Dsse(env),
            pinning: clean_pinning(),
            policy: policy(1),
            trust_store: &store,
        });
        assert_eq!(
            report.checks.provenance.builder_identity,
            CheckStatus::IdentityMismatch
        );
        assert_eq!(report.policy_result, PolicyResult::Fail);
    }

    #[test]
    fn floating_source_ref_is_policy_not_satisfied() {
        let store = TrustStore::new();
        let mut pinning = clean_pinning();
        pinning.floating_source_ref = true;
        let report = verify_supply_chain(VerifyInput {
            subject: subject(),
            expected_artifact_digest: None,
            provenance: ProvenanceInput::None,
            pinning,
            policy: policy(0),
            trust_store: &store,
        });
        assert_eq!(
            report.checks.pinning.no_floating_source_ref,
            CheckStatus::PolicyNotSatisfied
        );
        assert_eq!(report.policy_result, PolicyResult::Fail);
    }

    #[test]
    fn lockfile_digest_mismatch_fails() {
        let store = TrustStore::new();
        let mut pinning = clean_pinning();
        pinning.lockfile_digest = Some(
            "sha256:9999999999999999999999999999999999999999999999999999999999999999".to_string(),
        );
        let report = verify_supply_chain(VerifyInput {
            subject: subject(),
            expected_artifact_digest: None,
            provenance: ProvenanceInput::None,
            pinning,
            policy: policy(0),
            trust_store: &store,
        });
        assert_eq!(
            report.checks.pinning.lockfile_subject_matches_artifact,
            CheckStatus::Failed
        );
        assert_eq!(report.policy_result, PolicyResult::Fail);
    }

    #[test]
    fn trust_root_missing_is_trust_root_unavailable() {
        // Sign with a key that is NOT in the (empty) trust store.
        let sk = SigningKey::from_bytes(&[1u8; 32]);
        let store = TrustStore::new();
        let env = signed_dsse(
            &sk,
            "sha256:notinstore",
            &statement_json(hex_of(ARTIFACT_DIGEST), SLSA_PROVENANCE_PREDICATE, BUILDER),
        );
        let report = verify_supply_chain(VerifyInput {
            subject: subject(),
            expected_artifact_digest: None,
            provenance: ProvenanceInput::Dsse(env),
            pinning: clean_pinning(),
            policy: policy(1),
            trust_store: &store,
        });
        assert_eq!(
            report.checks.provenance.dsse_signature,
            CheckStatus::TrustRootUnavailable
        );
        // Signature unverifiable -> cannot reach L2; declared L1 unmet -> not clean.
        assert!(!is_clean(&report));
    }

    #[test]
    fn unsupported_predicate_is_unsupported_format() {
        let sk = SigningKey::from_bytes(&[2u8; 32]);
        let (store, key_id) = trust_with(&sk.verifying_key());
        let env = signed_dsse(
            &sk,
            &key_id,
            &statement_json(
                hex_of(ARTIFACT_DIGEST),
                "https://example/other-predicate/v1",
                BUILDER,
            ),
        );
        let report = verify_supply_chain(VerifyInput {
            subject: subject(),
            expected_artifact_digest: None,
            provenance: ProvenanceInput::Dsse(env),
            pinning: clean_pinning(),
            policy: policy(1),
            trust_store: &store,
        });
        assert_eq!(
            report.checks.provenance.slsa_provenance,
            CheckStatus::UnsupportedFormat
        );
    }

    #[test]
    fn invalid_signature_is_failed_not_trust_root() {
        // Key IS in the store, but the signature bytes are corrupted -> Failed (not TrustRootUnavailable).
        let sk = SigningKey::from_bytes(&[8u8; 32]);
        let (store, key_id) = trust_with(&sk.verifying_key());
        let mut env = signed_dsse(
            &sk,
            &key_id,
            &statement_json(hex_of(ARTIFACT_DIGEST), SLSA_PROVENANCE_PREDICATE, BUILDER),
        );
        // Corrupt the signature (valid base64 of 64 zero bytes -> wrong signature).
        env.signatures[0].signature = BASE64.encode([0u8; 64]);
        let report = verify_supply_chain(VerifyInput {
            subject: subject(),
            expected_artifact_digest: None,
            provenance: ProvenanceInput::Dsse(env),
            pinning: clean_pinning(),
            policy: policy(1),
            trust_store: &store,
        });
        assert_eq!(report.checks.provenance.dsse_signature, CheckStatus::Failed);
    }

    #[test]
    fn carrier_is_value_free_and_vsa_mappable() {
        let store = TrustStore::new();
        let report = verify_supply_chain(VerifyInput {
            subject: subject(),
            expected_artifact_digest: None,
            provenance: ProvenanceInput::None,
            pinning: clean_pinning(),
            policy: policy(0),
            trust_store: &store,
        });
        let v = serde_json::to_value(&report).unwrap();
        // VSA-mappable shape: subject, declared/verified, policy_result, non_claims all present.
        assert_eq!(v["schema"], SCHEMA);
        assert!(v["subject"]["digest"].is_string());
        assert_eq!(v["declared"]["required_slsa_build_level"], "L0");
        assert_eq!(v["verified"]["slsa_build_level"], "L0");
        assert!(v["policy_result"].is_string());
        assert!(v["non_claims"].as_array().unwrap().len() >= 4);
    }

    #[test]
    fn sigstore_bundle_parse_failure_is_failed_never_verified() {
        // Hermetic (no fixture): a malformed Sigstore bundle must mark `sigstore_bundle` and every
        // dependent dimension Failed, the transparency extensions NotChecked, and never be `is_clean`.
        let store = TrustStore::new();
        let report = verify_supply_chain(VerifyInput {
            subject: subject(),
            expected_artifact_digest: None,
            provenance: ProvenanceInput::SigstoreBundle(Box::new(SigstoreBundleInput {
                bundle_json: b"not a bundle".to_vec(),
                fulcio_roots: vec![],
                fulcio_intermediates: vec![],
                rekor_trusted_root_json: b"{}".to_vec(),
                now_unix_secs: 1_750_000_000,
                expected_san: "x".to_string(),
                expected_issuer: "y".to_string(),
                rekor_requirement: TransparencyRequirement::Optional,
            })),
            pinning: clean_pinning(),
            policy: policy(0),
            trust_store: &store,
        });
        let p = &report.checks.provenance;
        assert_eq!(p.sigstore_bundle, CheckStatus::Failed);
        assert_eq!(p.cert_chain, CheckStatus::Failed);
        assert_eq!(p.identity, CheckStatus::Failed);
        assert_eq!(p.dsse_pae, CheckStatus::Failed);
        assert_eq!(p.rekor_inclusion, CheckStatus::Failed);
        assert_eq!(p.timestamp_freshness, CheckStatus::NotChecked);
        assert_eq!(p.consistency, CheckStatus::NotChecked);
        assert_eq!(p.witnessing, CheckStatus::NotChecked);
        assert_eq!(report.policy_result, PolicyResult::Fail);
        assert!(!is_clean(&report));
    }
}
