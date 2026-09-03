//! Wire contract of the shared claim vocabulary (ADR-048, decision 2).
//!
//! The two enums moved here from `assay-runner-schema` and `assay-evidence`, where each carried
//! `#[serde(rename_all = "snake_case")]`. Every artifact that already serialised one of the four
//! former paths must read back unchanged, so the spelling is pinned member by member rather than
//! left to the derive.

use assay_common::claim::{ClaimDecision, ClaimKind};

#[test]
fn shared_claim_vocabulary_keeps_its_wire_spelling() {
    for (value, wire) in [
        (ClaimDecision::Allowed, "\"allowed\""),
        (ClaimDecision::Degraded, "\"degraded\""),
        (ClaimDecision::Blocked, "\"blocked\""),
    ] {
        assert_eq!(serde_json::to_string(&value).unwrap(), wire);
        assert_eq!(serde_json::from_str::<ClaimDecision>(wire).unwrap(), value);
    }

    for (value, wire) in [
        (ClaimKind::PositiveExistence, "\"positive_existence\""),
        (ClaimKind::ExhaustiveSet, "\"exhaustive_set\""),
        (ClaimKind::BoundedNegative, "\"bounded_negative\""),
    ] {
        assert_eq!(serde_json::to_string(&value).unwrap(), wire);
        assert_eq!(serde_json::from_str::<ClaimKind>(wire).unwrap(), value);
    }
}

/// A fourth member would change what every decision table means (ADR-048, decision 6), so an
/// unknown spelling must be a deserialisation error, never a silent default.
#[test]
fn unknown_members_are_rejected_not_defaulted() {
    assert!(serde_json::from_str::<ClaimDecision>("\"unknown\"").is_err());
    assert!(serde_json::from_str::<ClaimKind>("\"unknown\"").is_err());
}
