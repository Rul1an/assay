//! The four public claim-vocabulary paths keep their wire spelling across ADR-048's move.
//!
//! This test names only paths that exist on both sides of the migration, so it is green before
//! the move and must stay green after it: it pins behaviour, not structure.

use assay_evidence::{CodingAgentClaimKind, CodingAgentGateDecision};
use assay_runner_schema::{ClaimGateDecision, CoverageClaimKind};

#[test]
fn every_public_path_keeps_the_snake_case_wire_spelling() {
    let runner_decisions = [
        (ClaimGateDecision::Allowed, "\"allowed\""),
        (ClaimGateDecision::Degraded, "\"degraded\""),
        (ClaimGateDecision::Blocked, "\"blocked\""),
    ];
    let evidence_decisions = [
        (CodingAgentGateDecision::Allowed, "\"allowed\""),
        (CodingAgentGateDecision::Degraded, "\"degraded\""),
        (CodingAgentGateDecision::Blocked, "\"blocked\""),
    ];
    for (value, wire) in runner_decisions {
        assert_eq!(serde_json::to_string(&value).unwrap(), wire);
        assert_eq!(
            serde_json::from_str::<ClaimGateDecision>(wire).unwrap(),
            value
        );
    }
    for (value, wire) in evidence_decisions {
        assert_eq!(serde_json::to_string(&value).unwrap(), wire);
        assert_eq!(
            serde_json::from_str::<CodingAgentGateDecision>(wire).unwrap(),
            value
        );
    }

    let runner_kinds = [
        (
            CoverageClaimKind::PositiveExistence,
            "\"positive_existence\"",
        ),
        (CoverageClaimKind::ExhaustiveSet, "\"exhaustive_set\""),
        (CoverageClaimKind::BoundedNegative, "\"bounded_negative\""),
    ];
    let evidence_kinds = [
        (
            CodingAgentClaimKind::PositiveExistence,
            "\"positive_existence\"",
        ),
        (CodingAgentClaimKind::ExhaustiveSet, "\"exhaustive_set\""),
        (
            CodingAgentClaimKind::BoundedNegative,
            "\"bounded_negative\"",
        ),
    ];
    for (value, wire) in runner_kinds {
        assert_eq!(serde_json::to_string(&value).unwrap(), wire);
        assert_eq!(
            serde_json::from_str::<CoverageClaimKind>(wire).unwrap(),
            value
        );
    }
    for (value, wire) in evidence_kinds {
        assert_eq!(serde_json::to_string(&value).unwrap(), wire);
        assert_eq!(
            serde_json::from_str::<CodingAgentClaimKind>(wire).unwrap(),
            value
        );
    }

    // The module path is public too, and `cargo semver-checks` reports it as its own item.
    let via_module = assay_evidence::coding_agent::CodingAgentGateDecision::Blocked;
    assert_eq!(serde_json::to_string(&via_module).unwrap(), "\"blocked\"");
    let via_module_kind = assay_evidence::coding_agent::CodingAgentClaimKind::ExhaustiveSet;
    assert_eq!(
        serde_json::to_string(&via_module_kind).unwrap(),
        "\"exhaustive_set\""
    );
}
