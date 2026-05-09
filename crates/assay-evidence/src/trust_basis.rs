use crate::bundle::VerifyLimits;
use anyhow::Result;
use std::io::Read;

mod canonical;
mod classifiers;
mod diff;
mod generation;
mod types;

pub use diff::{diff_trust_basis, duplicate_trust_basis_claim_ids};
pub use types::{
    TrustBasis, TrustBasisClaim, TrustBasisClaimLevelDiff, TrustBasisClaimMetadataDiff,
    TrustBasisClaimPresenceDiff, TrustBasisDiffClass, TrustBasisDiffReport, TrustBasisDiffSummary,
    TrustBasisOptions, TrustClaimBoundary, TrustClaimId, TrustClaimLevel, TrustClaimSource,
    TRUST_BASIS_DIFF_SCHEMA,
};

pub fn generate_trust_basis<R: Read>(
    reader: R,
    limits: VerifyLimits,
    options: TrustBasisOptions,
) -> Result<TrustBasis> {
    generation::generate_trust_basis(reader, limits, options)
}

pub fn to_canonical_json_bytes(trust_basis: &TrustBasis) -> Result<Vec<u8>> {
    canonical::to_canonical_json_bytes(trust_basis)
}

#[cfg(test)]
mod tests;
