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

mod provenance;
mod sigstore;
mod types;

#[cfg(test)]
mod tests;

pub use types::*;

pub const SCHEMA: &str = "assay.supply_chain_conformance.v0";
pub(super) const STATEMENT_TYPE_V1: &str = "https://in-toto.io/Statement/v1";
pub(super) const SLSA_PROVENANCE_PREDICATE: &str = "https://slsa.dev/provenance/v1";
pub(super) const DSSE_PAYLOAD_TYPE: &str = "application/vnd.in-toto+json";

fn hex_of(d: &str) -> &str {
    d.strip_prefix("sha256:").unwrap_or(d)
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
        Some(_) => CheckStatus::Failed,
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

/// All statuses EXCEPT transparency-extension dimensions. These are optional-by-default and gated by
/// `require_*`, so they are not swept by the generic "any pending -> Incomplete" rule.
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

/// Producer-side policy summary. Plimsoll applies the nuanced, policy-aware mapping; this is the
/// carrier's own coarse verdict.
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

    let prov = provenance::verify_provenance(&input);
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

/// A report is clean only when every applicable check is `verified` (`not_applicable` allowed).
pub fn is_clean(report: &SupplyChainConformance) -> bool {
    report.schema == SCHEMA
        && all_statuses(&report.checks)
            .iter()
            .all(|s| matches!(s, CheckStatus::Verified | CheckStatus::NotApplicable))
}
