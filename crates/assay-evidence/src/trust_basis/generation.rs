use super::classifiers::{
    classify_authorization_context, classify_containment_degradation, classify_delegation_context,
    classify_external_decision_receipt_boundary, classify_external_eval_receipt_boundary,
    classify_external_inventory_receipt_boundary, classify_pack_findings,
    classify_provenance_evidence, classify_signing_evidence,
};
use super::{
    TrustBasis, TrustBasisClaim, TrustBasisOptions, TrustClaimBoundary, TrustClaimId,
    TrustClaimLevel, TrustClaimSource,
};
use crate::bundle::{BundleReader, VerifyLimits};
use crate::lint::engine::lint_bundle_with_options;
use anyhow::{bail, Result};
use std::io::{Cursor, Read};

pub(super) fn generate_trust_basis<R: Read>(
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
                id: TrustClaimId::AuthorizationContextVisible,
                level: classify_authorization_context(&events),
                source: TrustClaimSource::CanonicalDecisionEvidence,
                boundary: TrustClaimBoundary::SupportedAuthProjectedFlowsOnly,
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
                id: TrustClaimId::ExternalEvalReceiptBoundaryVisible,
                level: classify_external_eval_receipt_boundary(&events),
                source: TrustClaimSource::ExternalEvidenceReceipt,
                boundary: TrustClaimBoundary::SupportedExternalEvalReceiptEventsOnly,
                note: None,
            },
            TrustBasisClaim {
                id: TrustClaimId::ExternalDecisionReceiptBoundaryVisible,
                level: classify_external_decision_receipt_boundary(&events),
                source: TrustClaimSource::ExternalDecisionReceipt,
                boundary: TrustClaimBoundary::SupportedExternalDecisionReceiptEventsOnly,
                note: None,
            },
            TrustBasisClaim {
                id: TrustClaimId::ExternalInventoryReceiptBoundaryVisible,
                level: classify_external_inventory_receipt_boundary(&events),
                source: TrustClaimSource::ExternalInventoryReceipt,
                boundary: TrustClaimBoundary::SupportedExternalInventoryReceiptEventsOnly,
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
