use crate::bundle::{BundleReader, VerifyLimits};
use crate::lint::engine::{lint_bundle_with_options, LintOptions, LintReportWithPacks};
use crate::types::EvidenceEvent;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrustClaimId {
    BundleVerified,
    SigningEvidencePresent,
    ProvenanceBackedClaimsPresent,
    DelegationContextVisible,
    ContainmentDegradationObserved,
    AppliedPackFindingsPresent,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrustClaimLevel {
    Verified,
    SelfReported,
    Inferred,
    Absent,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrustClaimSource {
    BundleVerification,
    BundleProofSurface,
    CanonicalDecisionEvidence,
    CanonicalEventPresence,
    PackExecutionResults,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TrustClaimBoundary {
    BundleWide,
    SupportedDelegatedFlowsOnly,
    SupportedContainmentFallbackPathsOnly,
    ProofSurfacesOnly,
    PackExecutionOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustBasisClaim {
    pub id: TrustClaimId,
    pub level: TrustClaimLevel,
    pub source: TrustClaimSource,
    pub boundary: TrustClaimBoundary,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustBasis {
    pub claims: Vec<TrustBasisClaim>,
}

#[derive(Debug, Clone, Default)]
pub struct TrustBasisOptions {
    pub lint: Option<LintOptions>,
}

pub fn generate_trust_basis<R: Read>(
    reader: R,
    limits: VerifyLimits,
    options: TrustBasisOptions,
) -> Result<TrustBasis> {
    let mut bundle_data = Vec::new();
    let mut limited_reader = reader.take(limits.max_bundle_bytes.saturating_add(1));
    limited_reader.read_to_end(&mut bundle_data)?;
    if bundle_data.len() as u64 > limits.max_bundle_bytes {
        bail!(
            "trust basis bundle exceeds compressed input limit of {} bytes",
            limits.max_bundle_bytes
        );
    }

    let bundle_reader = BundleReader::open_with_limits(Cursor::new(&bundle_data), limits)?;
    let events = bundle_reader.events_vec()?;

    let lint_result = match options.lint {
        Some(lint_options) if !lint_options.packs.is_empty() => Some(lint_bundle_with_options(
            Cursor::new(&bundle_data),
            limits,
            lint_options,
        )?),
        _ => None,
    };

    Ok(TrustBasis {
        claims: vec![
            TrustBasisClaim {
                id: TrustClaimId::BundleVerified,
                level: TrustClaimLevel::Verified,
                source: TrustClaimSource::BundleVerification,
                boundary: TrustClaimBoundary::BundleWide,
                note: None,
            },
            TrustBasisClaim {
                id: TrustClaimId::SigningEvidencePresent,
                level: classify_signing_evidence(&bundle_reader),
                source: TrustClaimSource::BundleProofSurface,
                boundary: TrustClaimBoundary::ProofSurfacesOnly,
                note: None,
            },
            TrustBasisClaim {
                id: TrustClaimId::ProvenanceBackedClaimsPresent,
                level: classify_provenance_evidence(&bundle_reader),
                source: TrustClaimSource::BundleProofSurface,
                boundary: TrustClaimBoundary::ProofSurfacesOnly,
                note: None,
            },
            TrustBasisClaim {
                id: TrustClaimId::DelegationContextVisible,
                level: classify_delegation_context(&events),
                source: TrustClaimSource::CanonicalDecisionEvidence,
                boundary: TrustClaimBoundary::SupportedDelegatedFlowsOnly,
                note: None,
            },
            TrustBasisClaim {
                id: TrustClaimId::ContainmentDegradationObserved,
                level: classify_containment_degradation(&events),
                source: TrustClaimSource::CanonicalEventPresence,
                boundary: TrustClaimBoundary::SupportedContainmentFallbackPathsOnly,
                note: None,
            },
            TrustBasisClaim {
                id: TrustClaimId::AppliedPackFindingsPresent,
                level: classify_pack_findings(lint_result.as_ref()),
                source: TrustClaimSource::PackExecutionResults,
                boundary: TrustClaimBoundary::PackExecutionOnly,
                note: None,
            },
        ],
    })
}

pub fn to_canonical_json_bytes(trust_basis: &TrustBasis) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"  ");
    let mut serializer = serde_json::Serializer::with_formatter(&mut output, formatter);
    trust_basis.serialize(&mut serializer)?;
    output.push(b'\n');
    Ok(output)
}

fn classify_signing_evidence(_bundle_reader: &BundleReader) -> TrustClaimLevel {
    // T1a v1 stays conservative: ordinary evidence bundles do not yet carry a
    // dedicated signed proof surface for runtime trust claims.
    TrustClaimLevel::Absent
}

fn classify_provenance_evidence(_bundle_reader: &BundleReader) -> TrustClaimLevel {
    // T1a v1 stays conservative: ordinary evidence bundles do not yet carry a
    // dedicated provenance-proof surface strong enough for this claim.
    TrustClaimLevel::Absent
}

fn classify_delegation_context(events: &[EvidenceEvent]) -> TrustClaimLevel {
    let has_supported_delegation = events.iter().any(|event| {
        event.type_ == "assay.tool.decision"
            && event
                .payload
                .get("delegated_from")
                .and_then(|value| value.as_str())
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false)
    });

    if has_supported_delegation {
        TrustClaimLevel::Verified
    } else {
        TrustClaimLevel::Absent
    }
}

fn classify_containment_degradation(events: &[EvidenceEvent]) -> TrustClaimLevel {
    if events
        .iter()
        .any(|event| event.type_ == "assay.sandbox.degraded")
    {
        TrustClaimLevel::Verified
    } else {
        TrustClaimLevel::Absent
    }
}

fn classify_pack_findings(lint_result: Option<&LintReportWithPacks>) -> TrustClaimLevel {
    let Some(lint_result) = lint_result else {
        return TrustClaimLevel::Absent;
    };

    let Some(pack_meta) = lint_result.pack_meta.as_ref() else {
        return TrustClaimLevel::Absent;
    };

    let prefixes: Vec<String> = pack_meta
        .packs
        .iter()
        .map(|pack| format!("{}@{}:", pack.name, pack.version))
        .collect();

    let has_pack_finding = lint_result.report.findings.iter().any(|finding| {
        prefixes
            .iter()
            .any(|prefix| finding.rule_id.starts_with(prefix))
    });

    if has_pack_finding {
        TrustClaimLevel::Verified
    } else {
        TrustClaimLevel::Absent
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::BundleWriter;
    use crate::lint::packs::load_pack;
    use chrono::{TimeZone, Utc};
    use serde_json::json;

    fn make_event(
        type_: &str,
        run_id: &str,
        seq: u64,
        payload: serde_json::Value,
    ) -> EvidenceEvent {
        let mut event =
            EvidenceEvent::new(type_, "urn:assay:test:trust-basis", run_id, seq, payload);
        event.time = Utc.timestamp_opt(1_700_000_000 + seq as i64, 0).unwrap();
        event
    }

    fn make_bundle(events: Vec<EvidenceEvent>) -> Vec<u8> {
        let mut buffer = Vec::new();
        let mut writer = BundleWriter::new(&mut buffer);
        for event in events {
            writer.add_event(event);
        }
        writer.finish().expect("bundle should finish");
        buffer
    }

    fn claim(trust_basis: &TrustBasis, id: TrustClaimId) -> &TrustBasisClaim {
        trust_basis
            .claims
            .iter()
            .find(|claim| claim.id == id)
            .expect("claim should exist")
    }

    #[test]
    fn trust_basis_always_emits_all_frozen_claims() {
        let bundle = make_bundle(vec![make_event(
            "assay.process.exec",
            "run_all_claims",
            0,
            json!({ "hits": 1 }),
        )]);

        let trust_basis = generate_trust_basis(
            Cursor::new(bundle),
            VerifyLimits::default(),
            TrustBasisOptions::default(),
        )
        .expect("trust basis should generate");

        assert_eq!(
            trust_basis
                .claims
                .iter()
                .map(|claim| (claim.id, claim.source, claim.boundary))
                .collect::<Vec<_>>(),
            vec![
                (
                    TrustClaimId::BundleVerified,
                    TrustClaimSource::BundleVerification,
                    TrustClaimBoundary::BundleWide,
                ),
                (
                    TrustClaimId::SigningEvidencePresent,
                    TrustClaimSource::BundleProofSurface,
                    TrustClaimBoundary::ProofSurfacesOnly,
                ),
                (
                    TrustClaimId::ProvenanceBackedClaimsPresent,
                    TrustClaimSource::BundleProofSurface,
                    TrustClaimBoundary::ProofSurfacesOnly,
                ),
                (
                    TrustClaimId::DelegationContextVisible,
                    TrustClaimSource::CanonicalDecisionEvidence,
                    TrustClaimBoundary::SupportedDelegatedFlowsOnly,
                ),
                (
                    TrustClaimId::ContainmentDegradationObserved,
                    TrustClaimSource::CanonicalEventPresence,
                    TrustClaimBoundary::SupportedContainmentFallbackPathsOnly,
                ),
                (
                    TrustClaimId::AppliedPackFindingsPresent,
                    TrustClaimSource::PackExecutionResults,
                    TrustClaimBoundary::PackExecutionOnly,
                ),
            ]
        );
        assert_eq!(
            trust_basis
                .claims
                .iter()
                .map(|claim| claim.level)
                .collect::<Vec<_>>(),
            vec![
                TrustClaimLevel::Verified,
                TrustClaimLevel::Absent,
                TrustClaimLevel::Absent,
                TrustClaimLevel::Absent,
                TrustClaimLevel::Absent,
                TrustClaimLevel::Absent,
            ]
        );
    }

    #[test]
    fn trust_basis_regeneration_is_byte_stable() {
        let bundle = make_bundle(vec![make_event(
            "assay.tool.decision",
            "run_stable",
            0,
            json!({
                "tool": "tool.commit",
                "decision": "allow",
                "delegated_from": "agent:planner"
            }),
        )]);

        let first = generate_trust_basis(
            Cursor::new(&bundle),
            VerifyLimits::default(),
            TrustBasisOptions::default(),
        )
        .expect("first trust basis");
        let second = generate_trust_basis(
            Cursor::new(&bundle),
            VerifyLimits::default(),
            TrustBasisOptions::default(),
        )
        .expect("second trust basis");

        assert_eq!(
            to_canonical_json_bytes(&first).expect("first json"),
            to_canonical_json_bytes(&second).expect("second json")
        );
    }

    #[test]
    fn trust_basis_detects_supported_delegation_and_degradation() {
        let bundle = make_bundle(vec![
            make_event(
                "assay.tool.decision",
                "run_signals",
                0,
                json!({
                    "tool": "tool.commit",
                    "decision": "allow",
                    "delegated_from": "agent:planner"
                }),
            ),
            make_event(
                "assay.sandbox.degraded",
                "run_signals",
                1,
                json!({
                    "reason_code": "policy_conflict",
                    "degradation_mode": "audit_fallback",
                    "component": "landlock"
                }),
            ),
        ]);

        let trust_basis = generate_trust_basis(
            Cursor::new(bundle),
            VerifyLimits::default(),
            TrustBasisOptions::default(),
        )
        .expect("trust basis should generate");

        assert_eq!(
            claim(&trust_basis, TrustClaimId::DelegationContextVisible).level,
            TrustClaimLevel::Verified
        );
        assert_eq!(
            claim(&trust_basis, TrustClaimId::ContainmentDegradationObserved).level,
            TrustClaimLevel::Verified
        );
    }

    #[test]
    fn trust_basis_keeps_signing_and_provenance_absent_despite_tempting_metadata() {
        let bundle = make_bundle(vec![make_event(
            "assay.tool.decision",
            "run_conservative",
            0,
            json!({
                "tool": "tool.commit",
                "decision": "allow",
                "signature": "pretend",
                "provenance": { "claimed": true }
            }),
        )]);

        let trust_basis = generate_trust_basis(
            Cursor::new(bundle),
            VerifyLimits::default(),
            TrustBasisOptions::default(),
        )
        .expect("trust basis should generate");

        assert_eq!(
            claim(&trust_basis, TrustClaimId::SigningEvidencePresent).level,
            TrustClaimLevel::Absent
        );
        assert_eq!(
            claim(&trust_basis, TrustClaimId::ProvenanceBackedClaimsPresent).level,
            TrustClaimLevel::Absent
        );
    }

    #[test]
    fn trust_basis_marks_pack_findings_only_when_explicit_pack_execution_finds_results() {
        let pack = load_pack("owasp-agentic-a3-a5-signal-followup").expect("pack should load");
        let bundle = make_bundle(vec![make_event(
            "assay.tool.decision",
            "run_pack_findings",
            0,
            json!({
                "tool": "tool.commit",
                "decision": "allow",
                "principal": "user:alice"
            }),
        )]);

        let trust_basis = generate_trust_basis(
            Cursor::new(bundle),
            VerifyLimits::default(),
            TrustBasisOptions {
                lint: Some(LintOptions {
                    packs: vec![pack],
                    max_results: Some(500),
                    bundle_path: Some("trust-basis-pack.tar.gz".to_string()),
                }),
            },
        )
        .expect("trust basis should generate");

        assert_eq!(
            claim(&trust_basis, TrustClaimId::AppliedPackFindingsPresent).level,
            TrustClaimLevel::Verified
        );
    }

    #[test]
    fn trust_basis_respects_max_bundle_bytes_before_verification() {
        let bundle = make_bundle(vec![make_event(
            "assay.tool.decision",
            "run_size_limit",
            0,
            json!({
                "tool": "tool.commit",
                "decision": "allow"
            }),
        )]);

        let err = generate_trust_basis(
            Cursor::new(bundle),
            VerifyLimits {
                max_bundle_bytes: 8,
                ..VerifyLimits::default()
            },
            TrustBasisOptions::default(),
        )
        .expect_err("trust basis generation should fail when compressed input exceeds limit");

        assert!(
            err.to_string()
                .contains("trust basis bundle exceeds compressed input limit"),
            "unexpected error: {err}"
        );
    }
}
