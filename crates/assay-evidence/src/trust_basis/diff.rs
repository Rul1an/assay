use super::{
    TrustBasis, TrustBasisClaim, TrustBasisClaimLevelDiff, TrustBasisClaimMetadataDiff,
    TrustBasisClaimPresenceDiff, TrustBasisDiffClass, TrustBasisDiffReport, TrustBasisDiffSummary,
    TrustClaimId, TrustClaimLevel, TRUST_BASIS_DIFF_SCHEMA,
};
use std::collections::{HashMap, HashSet};

/// Diff two Trust Basis artifacts by stable claim identity.
///
/// The CLI rejects duplicate claim IDs before calling this function. Library
/// callers still get deterministic output if duplicate IDs are present: the
/// first claim for each ID wins, and duplicate IDs are not repeated in diff
/// arrays.
pub fn diff_trust_basis(baseline: &TrustBasis, candidate: &TrustBasis) -> TrustBasisDiffReport {
    let baseline_by_id = first_claim_by_id(baseline);
    let candidate_by_id = first_claim_by_id(candidate);

    let mut regressed_claims = Vec::new();
    let mut improved_claims = Vec::new();
    let mut removed_claims = Vec::new();
    let mut metadata_changes = Vec::new();
    let mut unchanged_claim_count = 0;
    let mut seen_candidate_ids = HashSet::new();

    for baseline_claim in baseline_by_id.values() {
        let Some(candidate_claim) = candidate_by_id.get(&baseline_claim.id).copied() else {
            removed_claims.push(presence_diff_removed(baseline_claim));
            continue;
        };
        seen_candidate_ids.insert(candidate_claim.id);

        let baseline_rank = trust_claim_level_rank(baseline_claim.level);
        let candidate_rank = trust_claim_level_rank(candidate_claim.level);
        if candidate_rank < baseline_rank {
            regressed_claims.push(TrustBasisClaimLevelDiff {
                diff_class: TrustBasisDiffClass::Regressed,
                claim_id: baseline_claim.id,
                baseline_level: baseline_claim.level,
                candidate_level: candidate_claim.level,
            });
        } else if candidate_rank > baseline_rank {
            improved_claims.push(TrustBasisClaimLevelDiff {
                diff_class: TrustBasisDiffClass::Improved,
                claim_id: baseline_claim.id,
                baseline_level: baseline_claim.level,
                candidate_level: candidate_claim.level,
            });
        } else if baseline_claim.source != candidate_claim.source
            || baseline_claim.boundary != candidate_claim.boundary
            || baseline_claim.note != candidate_claim.note
        {
            metadata_changes.push(TrustBasisClaimMetadataDiff {
                diff_class: TrustBasisDiffClass::MetadataChanged,
                claim_id: baseline_claim.id,
                baseline_level: baseline_claim.level,
                candidate_level: candidate_claim.level,
                baseline_source: baseline_claim.source,
                candidate_source: candidate_claim.source,
                baseline_boundary: baseline_claim.boundary,
                candidate_boundary: candidate_claim.boundary,
                note_changed: baseline_claim.note != candidate_claim.note,
            });
        } else {
            unchanged_claim_count += 1;
        }
    }

    let mut added_claims: Vec<TrustBasisClaimPresenceDiff> = candidate_by_id
        .values()
        .copied()
        .filter(|claim| !seen_candidate_ids.contains(&claim.id))
        .map(presence_diff_added)
        .collect();

    regressed_claims.sort_by_key(|diff| claim_id_sort_key(diff.claim_id));
    improved_claims.sort_by_key(|diff| claim_id_sort_key(diff.claim_id));
    removed_claims.sort_by_key(|diff| claim_id_sort_key(diff.claim_id));
    added_claims.sort_by_key(|diff| claim_id_sort_key(diff.claim_id));
    metadata_changes.sort_by_key(|diff| claim_id_sort_key(diff.claim_id));

    let summary = TrustBasisDiffSummary {
        regressed_claims: regressed_claims.len(),
        improved_claims: improved_claims.len(),
        removed_claims: removed_claims.len(),
        added_claims: added_claims.len(),
        metadata_changes: metadata_changes.len(),
        unchanged_claim_count,
        has_regressions: !regressed_claims.is_empty() || !removed_claims.is_empty(),
    };

    TrustBasisDiffReport {
        schema: TRUST_BASIS_DIFF_SCHEMA.to_string(),
        claim_identity: "claim.id".to_string(),
        level_order: vec![
            TrustClaimLevel::Absent,
            TrustClaimLevel::Inferred,
            TrustClaimLevel::SelfReported,
            TrustClaimLevel::Verified,
        ],
        summary,
        regressed_claims,
        improved_claims,
        removed_claims,
        added_claims,
        metadata_changes,
        unchanged_claim_count,
    }
}

fn first_claim_by_id(trust_basis: &TrustBasis) -> HashMap<TrustClaimId, &TrustBasisClaim> {
    let mut by_id = HashMap::new();
    for claim in &trust_basis.claims {
        by_id.entry(claim.id).or_insert(claim);
    }
    by_id
}

fn presence_diff_removed(claim: &TrustBasisClaim) -> TrustBasisClaimPresenceDiff {
    TrustBasisClaimPresenceDiff {
        diff_class: TrustBasisDiffClass::Removed,
        claim_id: claim.id,
        baseline_level: Some(claim.level),
        candidate_level: None,
        baseline_source: Some(claim.source),
        candidate_source: None,
        baseline_boundary: Some(claim.boundary),
        candidate_boundary: None,
        baseline_note: claim.note.clone(),
        candidate_note: None,
    }
}

fn presence_diff_added(claim: &TrustBasisClaim) -> TrustBasisClaimPresenceDiff {
    TrustBasisClaimPresenceDiff {
        diff_class: TrustBasisDiffClass::Added,
        claim_id: claim.id,
        baseline_level: None,
        candidate_level: Some(claim.level),
        baseline_source: None,
        candidate_source: Some(claim.source),
        baseline_boundary: None,
        candidate_boundary: Some(claim.boundary),
        baseline_note: None,
        candidate_note: claim.note.clone(),
    }
}

pub fn duplicate_trust_basis_claim_ids(trust_basis: &TrustBasis) -> Vec<TrustClaimId> {
    let mut seen = HashSet::new();
    let mut duplicates: Vec<TrustClaimId> = trust_basis
        .claims
        .iter()
        .filter_map(|claim| {
            if seen.insert(claim.id) {
                None
            } else {
                Some(claim.id)
            }
        })
        .collect();
    duplicates.sort_by_key(|id| claim_id_sort_key(*id));
    duplicates.dedup();
    duplicates
}

fn claim_id_sort_key(id: TrustClaimId) -> String {
    match serde_json::to_value(id).expect("TrustClaimId serialization should succeed") {
        serde_json::Value::String(value) => value,
        _ => unreachable!("TrustClaimId should serialize as a string"),
    }
}

fn trust_claim_level_rank(level: TrustClaimLevel) -> u8 {
    match level {
        TrustClaimLevel::Absent => 0,
        TrustClaimLevel::Inferred => 1,
        TrustClaimLevel::SelfReported => 2,
        TrustClaimLevel::Verified => 3,
    }
}
