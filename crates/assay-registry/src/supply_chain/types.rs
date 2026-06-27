use serde::{Deserialize, Serialize};

use crate::trust::TrustStore;
use crate::types::DsseEnvelope;

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
    /// A dimension that is relevant but deliberately NOT verified in this slice.
    NotChecked,
}

impl CheckStatus {
    pub(super) fn is_blocking(self) -> bool {
        matches!(
            self,
            CheckStatus::Failed
                | CheckStatus::SubjectDigestMismatch
                | CheckStatus::IdentityMismatch
                | CheckStatus::PolicyNotSatisfied
        )
    }

    pub(super) fn is_pending(self) -> bool {
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
/// from an identified builder verified against the pinned trust root.
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
    pub cert_chain: CheckStatus,
    pub identity: CheckStatus,
    pub dsse_pae: CheckStatus,
    pub timestamp_freshness: CheckStatus,
    pub consistency: CheckStatus,
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

/// Provenance encountered on the artifact.
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

/// A keyless Sigstore DSSE bundle plus the PINNED trust material needed to verify it offline.
pub struct SigstoreBundleInput {
    pub bundle_json: Vec<u8>,
    pub fulcio_roots: Vec<Vec<u8>>,
    pub fulcio_intermediates: Vec<Vec<u8>>,
    pub rekor_trusted_root_json: Vec<u8>,
    pub now_unix_secs: u64,
    pub expected_san: String,
    pub expected_issuer: String,
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
    pub require_rekor_inclusion: bool,
    pub require_timestamp_freshness: bool,
    pub require_consistency: bool,
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
