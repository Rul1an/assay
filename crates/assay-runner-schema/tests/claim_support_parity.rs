//! The health half of the claim-support rule, held to its own contract.
//!
//! `assay-evidence`'s `tests/claim_gate_parity.rs` pins two Rust vocabularies against each other.
//! This file pins one vocabulary against the thing it claims to be: a table that *derives* from
//! `RunnerClaimGate` rather than restating it, is total over both enums, and carries the asymmetry
//! that makes the rule worth having.
//!
//! The one substantive assertion here is `absence_is_never_more_permissive_than_occurrence`. It is
//! true today by construction, and it was never checked. A gate that lets an absence claim through
//! where it degrades an occurrence claim has inverted the rule, and nothing in the crate would have
//! noticed.

use assay_runner_schema::{
    all_claim_kinds, all_verdicts, claim_support, claim_support_table, permissiveness,
    ClaimGateDecision, ClaimSupport, ClaimSupportScope, CoverageClaimKind, CoverageDescriptor,
    RunnerClaimGate, RunnerFidelityVerdict, CLAIM_SUPPORT_PARITY_SCHEMA,
};

fn decided(verdict: RunnerFidelityVerdict, kind: CoverageClaimKind) -> Option<ClaimGateDecision> {
    match claim_support(verdict, kind).support {
        ClaimSupport::Decided(decision) => Some(decision),
        ClaimSupport::NotModelled => None,
    }
}

#[test]
fn table_is_total_over_both_vocabularies() {
    let table = claim_support_table();
    assert_eq!(table.schema, CLAIM_SUPPORT_PARITY_SCHEMA);
    assert_eq!(
        table.rows.len(),
        all_verdicts().len() * all_claim_kinds().len()
    );

    for verdict in all_verdicts() {
        for kind in all_claim_kinds() {
            let matches = table
                .rows
                .iter()
                .filter(|row| row.verdict == verdict && row.kind == kind)
                .count();
            assert_eq!(
                matches, 1,
                "expected exactly one row for {verdict:?}/{kind:?}"
            );
        }
    }
}

/// The table must stay a *view* of the gate. If someone later inlines the fifteen decisions as
/// literals, this fails, which is the whole reason the module exists.
#[test]
fn table_derives_from_the_gate_rather_than_restating_it() {
    for verdict in all_verdicts() {
        let gate = RunnerClaimGate::for_verdict(verdict);
        assert_eq!(
            decided(verdict, CoverageClaimKind::PositiveExistence),
            Some(gate.measured_positive_claims),
            "positive row drifted from the gate at {verdict:?}"
        );
        assert_eq!(
            decided(verdict, CoverageClaimKind::BoundedNegative),
            Some(gate.bounded_negative_claims),
            "negative row drifted from the gate at {verdict:?}"
        );
    }
}

/// The asymmetry, asserted rather than assumed. Any class or health state may support an occurrence
/// claim at its ceiling; only a state that cannot be hiding something supports absence. A row where
/// absence is looser than occurrence would mean silence counted for more than a sighting.
#[test]
fn absence_is_never_more_permissive_than_occurrence() {
    for verdict in all_verdicts() {
        let positive = decided(verdict, CoverageClaimKind::PositiveExistence)
            .expect("positive existence is modelled for every verdict");
        let negative = decided(verdict, CoverageClaimKind::BoundedNegative)
            .expect("bounded negative is modelled for every verdict");

        assert!(
            permissiveness(negative) <= permissiveness(positive),
            "{verdict:?}: absence ({negative:?}) is more permissive than occurrence ({positive:?})"
        );
    }
}

/// And the asymmetry has to *bite* somewhere, or the assertion above is satisfied by a gate that
/// treats the two kinds identically everywhere.
#[test]
fn the_asymmetry_is_strict_for_at_least_one_verdict() {
    let strict = all_verdicts().into_iter().filter(|verdict| {
        let positive = decided(*verdict, CoverageClaimKind::PositiveExistence).unwrap();
        let negative = decided(*verdict, CoverageClaimKind::BoundedNegative).unwrap();
        permissiveness(negative) < permissiveness(positive)
    });

    let strict: Vec<_> = strict.collect();
    assert!(
        strict.contains(&RunnerFidelityVerdict::Clipped),
        "dropped records must separate the two claim kinds; strict at {strict:?}"
    );
}

/// The gap is recorded, not inferred. `ExhaustiveSet` has no field on the fidelity gate, and the
/// table says so instead of borrowing a neighbouring field's answer.
#[test]
fn exhaustive_set_is_reported_unmodelled_for_every_verdict() {
    for verdict in all_verdicts() {
        let row = claim_support(verdict, CoverageClaimKind::ExhaustiveSet);
        assert_eq!(
            row.support,
            ClaimSupport::NotModelled,
            "{verdict:?}: exhaustive set must not borrow another field's decision"
        );
        assert!(
            row.rule.contains("coverage_descriptor"),
            "the unmodelled row must name the gate that does model the kind, got {:?}",
            row.rule
        );
    }
}

/// ...and the kind is real, so the gap is in this gate specifically rather than in the vocabulary.
/// A full descriptor allows an exhaustive claim; a partial one degrades it. Neither answer exists
/// on the fidelity side, which is exactly what makes the `NotModelled` row load-bearing.
#[test]
fn the_coverage_gate_models_the_kind_this_table_does_not() {
    let full = CoverageDescriptor {
        schema: assay_runner_schema::COVERAGE_DESCRIPTOR_SCHEMA.to_string(),
        dimension: assay_runner_schema::EffectDimension::Filesystem,
        method: "complete filesystem observation".to_string(),
        observes: vec!["every path open".to_string()],
        known_blind_spots: vec![],
        completeness: assay_runner_schema::CoverageCompleteness::Full,
    };
    assert_eq!(
        full.claim_decision(CoverageClaimKind::ExhaustiveSet)
            .decision,
        ClaimGateDecision::Allowed
    );

    let partial = CoverageDescriptor::filesystem_open_syscall_only();
    assert_eq!(
        partial
            .claim_decision(CoverageClaimKind::ExhaustiveSet)
            .decision,
        ClaimGateDecision::Degraded
    );
}

/// Scope marks the *cause*, not the decision. `CorrelationPartial` reaches the same positive and
/// negative decisions as `Clipped` and is still the one verdict a class-half rule cannot cause,
/// because cgroup correlation is a runner-internal binding property.
#[test]
fn only_correlation_partial_is_health_input_only() {
    for verdict in all_verdicts() {
        for kind in all_claim_kinds() {
            let expected = if verdict == RunnerFidelityVerdict::CorrelationPartial {
                ClaimSupportScope::HealthInputOnly
            } else {
                ClaimSupportScope::Shared
            };
            assert_eq!(
                claim_support(verdict, kind).scope,
                expected,
                "scope wrong at {verdict:?}/{kind:?}"
            );
        }
    }

    // The decisions coincide with Clipped, which is why scope has to be stated separately.
    for kind in [
        CoverageClaimKind::PositiveExistence,
        CoverageClaimKind::BoundedNegative,
    ] {
        assert_eq!(
            decided(RunnerFidelityVerdict::CorrelationPartial, kind),
            decided(RunnerFidelityVerdict::Clipped, kind),
            "{kind:?}: if these ever diverge, the scope note needs rewriting"
        );
    }
}

/// The table travels to another language, so it has to survive a round trip unchanged.
#[test]
fn table_round_trips_through_json() {
    let table = claim_support_table();
    let encoded = serde_json::to_string(&table).expect("table serializes");
    let decoded: assay_runner_schema::ClaimSupportParityTable =
        serde_json::from_str(&encoded).expect("table deserializes");
    assert_eq!(decoded, table);
}

/// Non-claims are part of the contract: a reader must not take this table for a statement about
/// observer classes, which is the half that does not exist yet.
#[test]
fn table_declares_what_it_does_not_answer() {
    let table = claim_support_table();
    for expected in [
        "parity_no_observer_class_typing",
        "parity_no_blinding_cost_verdict",
        "parity_no_aggregate_claim_score",
        "parity_no_coverage_descriptor_replacement",
    ] {
        assert!(
            table.non_claims.iter().any(|claim| claim == expected),
            "missing non-claim {expected}"
        );
    }
}
