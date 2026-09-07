//! The CAP-1 relying-party gate, pinned three ways.
//!
//! 1. REFERENCE BASELINE (`github.com/Rul1an/cap1-conforming-but-misleading`, run of
//!    2026-09-07): decisions are checked for all three claim kinds on the 20 main vectors;
//!    rules fired are checked for bounded-negative only. Explicit Assay deviations for EV-01b
//!    and the five upstream positive vectors preserve the historical reference bytes.
//! 2. SHAPE PARITY with `coding_agent_claim_decision` on the two shapes that overlap: partial
//!    coverage (pair 03) and self-report (pair 06). The claim-kind asymmetry is the runner
//!    substrate's rule, and the CAP-1 gate must not silently mean something different.
//! 3. MUTATION: silence each of the seven rules; its own vectors must flip to `Allowed`, every
//!    honest twin must stay `Allowed`, every other vector must stay not-allowed. A rule whose
//!    silence changes nothing is decoration.
//!
//! Conformance of the fixtures to CAP-1 itself is NOT re-checked here (this crate has no CAP-1
//! verifier and claims none); the Python reference checks it under the author's own verifiers
//! and the fixtures are pinned from that run.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use assay_evidence::{
    cap1_claim_decision, cap1_claim_decision_with, coding_agent_claim_decision, Cap1Document,
    Cap1RelyingPartyContext, Cap1Rule, CodingAgentClaimKind, CodingAgentCoverageState,
    CodingAgentGateDecision, CodingAgentSourceClass,
};
use serde::Deserialize;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cap1")
}

#[derive(Deserialize)]
struct ContextFile {
    default: Cap1RelyingPartyContext,
    #[serde(default)]
    per_vector: BTreeMap<String, serde_json::Value>,
}

/// The Python reference merges per-vector overrides over the default; do the same, field-wise.
fn context_for(file: &ContextFile, id: &str) -> Cap1RelyingPartyContext {
    let mut base = serde_json::to_value(&file.default).expect("context serialises");
    if let Some(over) = file.per_vector.get(id) {
        let (Some(b), Some(o)) = (base.as_object_mut(), over.as_object()) else {
            panic!("context objects");
        };
        for (k, v) in o {
            b.insert(k.clone(), v.clone());
        }
    }
    serde_json::from_value(base).expect("merged context parses")
}

#[derive(Deserialize)]
struct Expected {
    vectors: BTreeMap<String, ExpectedVector>,
    upstream_positive: BTreeMap<String, ExpectedUpstream>,
}

#[derive(Deserialize)]
struct ExpectedVector {
    bounded_negative: String,
    exhaustive_set: String,
    positive_existence: String,
    rules_fired_bounded_negative: Vec<String>,
}

#[derive(Deserialize)]
struct ExpectedUpstream {
    bounded_negative: String,
    rules_fired_bounded_negative: Vec<String>,
}

fn load_doc(p: &Path) -> Cap1Document {
    serde_json::from_str(&fs::read_to_string(p).expect("read vector")).expect("vector parses")
}

fn load_context() -> ContextFile {
    let raw = fs::read_to_string(fixtures().join("context.json")).expect("read context");
    // The file carries a leading `_comment` the Rust struct does not; strip it before parsing.
    let mut v: serde_json::Value = serde_json::from_str(&raw).expect("context json");
    v.as_object_mut().expect("object").remove("_comment");
    serde_json::from_value(v).expect("context parses")
}

fn load_expected() -> Expected {
    serde_json::from_str(
        &fs::read_to_string(fixtures().join("expected.json")).expect("read expected"),
    )
    .expect("expected parses")
}

fn decision_str(d: CodingAgentGateDecision) -> &'static str {
    match d {
        CodingAgentGateDecision::Allowed => "allowed",
        CodingAgentGateDecision::Degraded => "degraded",
        CodingAgentGateDecision::Blocked => "blocked",
    }
}

fn vector_ids() -> Vec<String> {
    let mut ids: Vec<String> = fs::read_dir(fixtures().join("vectors"))
        .expect("vectors dir")
        .map(|e| {
            e.expect("entry")
                .file_name()
                .to_string_lossy()
                .trim_end_matches(".json")
                .to_string()
        })
        .collect();
    ids.sort();
    ids
}

#[test]
fn every_fixture_parses_under_the_closed_schema() {
    let ids = vector_ids();
    assert_eq!(ids.len(), 20, "6 MV + 6 HV + 8 EV");
    for id in &ids {
        let _ = load_doc(&fixtures().join("vectors").join(format!("{id}.json")));
    }
    for i in 1..=5 {
        let _ = load_doc(&fixtures().join("upstream").join(format!("PV-0{i}.json")));
    }
}

#[test]
fn reference_baseline_with_explicit_assay_population_deviations() {
    let ctx = load_context();
    let expected = load_expected();
    for id in vector_ids() {
        let doc = load_doc(&fixtures().join("vectors").join(format!("{id}.json")));
        let c = context_for(&ctx, &id);
        let want = expected
            .vectors
            .get(&id)
            .unwrap_or_else(|| panic!("expected row for {id}"));
        let neg = cap1_claim_decision(&doc, CodingAgentClaimKind::BoundedNegative, &c);
        let exh = cap1_claim_decision(&doc, CodingAgentClaimKind::ExhaustiveSet, &c);
        let pos = cap1_claim_decision(&doc, CodingAgentClaimKind::PositiveExistence, &c);
        assert_eq!(
            decision_str(neg.decision),
            want.bounded_negative,
            "{id} bounded_negative"
        );
        assert_eq!(
            decision_str(exh.decision),
            want.exhaustive_set,
            "{id} exhaustive_set"
        );
        assert_eq!(
            decision_str(pos.decision),
            want.positive_existence,
            "{id} positive_existence"
        );
        let fired: Vec<&str> = neg
            .rules_fired()
            .into_iter()
            .map(Cap1Rule::as_str)
            .collect();
        if id == "EV-01b" {
            // Unknown digest also lacks an owned population: two independent missing inputs.
            assert_eq!(
                want.rules_fired_bounded_negative,
                vec![Cap1Rule::DenominatorPrecommitted.as_str()]
            );
            assert_eq!(
                fired,
                vec![
                    Cap1Rule::DenominatorPrecommitted.as_str(),
                    Cap1Rule::PopulationGranularity.as_str()
                ],
                "{id} Assay rules"
            );
        } else {
            assert_eq!(fired, want.rules_fired_bounded_negative, "{id} rules fired");
        }
    }
    // Historical Python bytes are unchanged. These are Assay expectations, not a new
    // reference run: each PV context lacks the relying party's catalogue unit set.
    let deviations = [
        ("PV-01", "degraded", vec![Cap1Rule::PopulationGranularity]),
        (
            "PV-02",
            "blocked",
            vec![
                Cap1Rule::ApplicabilityConfirmable,
                Cap1Rule::AbsenceBoundedByExaminedUnits,
                Cap1Rule::PopulationGranularity,
            ],
        ),
        (
            "PV-03",
            "blocked",
            vec![
                Cap1Rule::AbsenceBoundedByExaminedUnits,
                Cap1Rule::PopulationGranularity,
            ],
        ),
        (
            "PV-04",
            "degraded",
            vec![
                Cap1Rule::WithheldResolvable,
                Cap1Rule::PopulationGranularity,
            ],
        ),
        (
            "PV-05",
            "degraded",
            vec![
                Cap1Rule::DenominatorPrecommitted,
                Cap1Rule::PopulationGranularity,
            ],
        ),
    ];
    assert_eq!(deviations.len(), expected.upstream_positive.len());
    for (id, decision, rules) in deviations {
        let historical = &expected.upstream_positive[id];
        assert_eq!(
            historical.bounded_negative,
            if id == "PV-01" { "allowed" } else { decision }
        );
        let unchanged_rules: Vec<_> = rules
            .iter()
            .copied()
            .filter(|r| *r != Cap1Rule::PopulationGranularity)
            .map(Cap1Rule::as_str)
            .collect();
        assert_eq!(
            historical.rules_fired_bounded_negative, unchanged_rules,
            "{id} historical rules"
        );
        let doc = load_doc(&fixtures().join("upstream").join(format!("{id}.json")));
        let result = cap1_claim_decision(
            &doc,
            CodingAgentClaimKind::BoundedNegative,
            &context_for(&ctx, id),
        );
        assert_eq!(
            decision_str(result.decision),
            decision,
            "{id} Assay decision"
        );
        assert_eq!(result.rules_fired(), rules, "{id} Assay rules");
    }
}

#[test]
fn honest_twins_are_allowed_and_everything_else_is_not() {
    let ctx = load_context();
    for id in vector_ids() {
        let doc = load_doc(&fixtures().join("vectors").join(format!("{id}.json")));
        let c = context_for(&ctx, &id);
        let neg = cap1_claim_decision(&doc, CodingAgentClaimKind::BoundedNegative, &c);
        let pos = cap1_claim_decision(&doc, CodingAgentClaimKind::PositiveExistence, &c);
        if id.starts_with("HV") {
            assert_eq!(neg.decision, CodingAgentGateDecision::Allowed, "{id}");
        } else {
            assert_ne!(neg.decision, CodingAgentGateDecision::Allowed, "{id}");
        }
        // The asymmetry: a positive claim is never blocked by coverage alone.
        assert_ne!(
            pos.decision,
            CodingAgentGateDecision::Blocked,
            "{id} positive"
        );
    }
}

/// One rule, one function: the two shapes this gate shares with `coding_agent_claim_decision`
/// must decide the same way, or the CAP-1 gate silently means something different.
#[test]
fn shape_parity_with_the_coding_agent_gate() {
    let ctx = load_context();
    let kinds = [
        CodingAgentClaimKind::PositiveExistence,
        CodingAgentClaimKind::ExhaustiveSet,
        CodingAgentClaimKind::BoundedNegative,
    ];

    // Pair 03: partial coverage over an observed-in-path source.
    let partial = load_doc(&fixtures().join("vectors/MV-03.json"));
    let c = context_for(&ctx, "MV-03");
    for kind in kinds {
        let ours = cap1_claim_decision(&partial, kind, &c).decision;
        let theirs = coding_agent_claim_decision(
            CodingAgentSourceClass::BoundaryObserved,
            CodingAgentCoverageState::Partial,
            kind,
        )
        .decision;
        assert_eq!(ours, theirs, "partial coverage, {kind:?}");
    }

    // Pair 06: the account comes from the subject.
    let selfrep = load_doc(&fixtures().join("vectors/MV-06.json"));
    let c = context_for(&ctx, "MV-06");
    for kind in kinds {
        let ours = cap1_claim_decision(&selfrep, kind, &c).decision;
        let theirs = coding_agent_claim_decision(
            CodingAgentSourceClass::ProducerReported,
            CodingAgentCoverageState::SelfReported,
            kind,
        )
        .decision;
        assert_eq!(ours, theirs, "self-reported, {kind:?}");
    }
}

/// CAP-1 section 7.2, applied to the consumer: rules that are never exercised are decoration.
#[test]
fn every_rule_is_killed_under_mutation() {
    let ctx = load_context();
    let ids = vector_ids();
    let docs: Vec<(String, Cap1Document, Cap1RelyingPartyContext)> = ids
        .iter()
        .map(|id| {
            (
                id.clone(),
                load_doc(&fixtures().join("vectors").join(format!("{id}.json"))),
                context_for(&ctx, id),
            )
        })
        .collect();
    let baseline: BTreeMap<&str, CodingAgentGateDecision> = docs
        .iter()
        .map(|(id, d, c)| {
            (
                id.as_str(),
                cap1_claim_decision(d, CodingAgentClaimKind::BoundedNegative, c).decision,
            )
        })
        .collect();

    let mut kills = 0;
    for rule in Cap1Rule::ALL {
        let mut flipped = Vec::new();
        let mut twins_ok = true;
        let mut others_ok = true;
        let mutant: BTreeMap<&str, CodingAgentGateDecision> = docs
            .iter()
            .map(|(id, d, c)| {
                (
                    id.as_str(),
                    cap1_claim_decision_with(d, CodingAgentClaimKind::BoundedNegative, c, &[rule])
                        .decision,
                )
            })
            .collect();
        for (id, dec) in &mutant {
            let allowed = *dec == CodingAgentGateDecision::Allowed;
            if id.starts_with("HV") {
                twins_ok &= allowed;
            } else if baseline[id] != CodingAgentGateDecision::Allowed && allowed {
                flipped.push(*id);
            }
        }
        let expected_flips: &[&str] = match rule {
            Cap1Rule::DenominatorPrecommitted => &["EV-01a", "MV-01"],
            Cap1Rule::ApplicabilityConfirmable => &["EV-02b", "MV-02"],
            Cap1Rule::AbsenceBoundedByExaminedUnits => &["MV-03"],
            Cap1Rule::WithheldResolvable => &["MV-04"],
            Cap1Rule::PopulationGranularity => &["EV-05a", "EV-05b", "EV-05c", "MV-05"],
            Cap1Rule::ProducerIndependentOfSubject => &["EV-06", "MV-06"],
            Cap1Rule::UnitIdentityUnique => &["EV-02"],
        };
        assert_eq!(
            flipped,
            expected_flips,
            "{} exact affected vectors",
            rule.as_str()
        );
        for (id, dec) in &mutant {
            if !id.starts_with("HV") && !expected_flips.contains(id) {
                others_ok &= *dec != CodingAgentGateDecision::Allowed;
            }
        }
        let killed = !flipped.is_empty() && twins_ok && others_ok;
        assert!(
            killed,
            "{} survived: flipped={flipped:?} twins_ok={twins_ok} others_ok={others_ok}",
            rule.as_str()
        );
        kills += 1;
    }
    assert_eq!(kills, Cap1Rule::ALL.len());
    // EV-01b now has independent C1 and C5 gaps. Neither single-rule mutation may allow it;
    // removing both does, explicitly separating this combined case from each rule's twins.
    let (id, doc, ctx) = docs.iter().find(|(id, _, _)| id == "EV-01b").unwrap();
    for disabled in [
        Cap1Rule::DenominatorPrecommitted,
        Cap1Rule::PopulationGranularity,
    ] {
        assert_eq!(
            cap1_claim_decision_with(doc, CodingAgentClaimKind::BoundedNegative, ctx, &[disabled])
                .decision,
            CodingAgentGateDecision::Degraded,
            "{id} surviving independent gap"
        );
    }
    assert_eq!(
        cap1_claim_decision_with(
            doc,
            CodingAgentClaimKind::BoundedNegative,
            ctx,
            &[
                Cap1Rule::DenominatorPrecommitted,
                Cap1Rule::PopulationGranularity
            ]
        )
        .decision,
        CodingAgentGateDecision::Allowed,
        "{id} both guards removed"
    );
}

#[test]
fn missing_catalogue_population_is_not_allowed() {
    use CodingAgentClaimKind as K;
    use CodingAgentGateDecision as D;
    let contexts = load_context();
    let doc = load_doc(&fixtures().join("vectors/MV-05.json"));
    let mut ctx = context_for(&contexts, "MV-05");
    assert_eq!(
        cap1_claim_decision(&doc, K::BoundedNegative, &ctx).decision,
        D::Blocked
    );
    ctx.catalogue_units.clear();
    for kind in [K::BoundedNegative, K::ExhaustiveSet] {
        let result = cap1_claim_decision(&doc, kind, &ctx);
        assert_eq!(
            result.decision,
            D::Degraded,
            "missing owned population, {kind:?}"
        );
        assert_eq!(result.rules_fired(), vec![Cap1Rule::PopulationGranularity]);
        assert_eq!(
            serde_json::to_value(result.findings[0].gap).unwrap(),
            "population_unavailable"
        );
    }
    assert_eq!(
        cap1_claim_decision(&doc, K::PositiveExistence, &ctx).decision,
        D::Allowed
    );
    let honest = load_doc(&fixtures().join("vectors/HV-05.json"));
    assert_eq!(
        cap1_claim_decision(
            &honest,
            K::BoundedNegative,
            &context_for(&contexts, "HV-05")
        )
        .decision,
        D::Allowed
    );
}

#[test]
fn missing_absence_support_cannot_allow_nonpositive_claims() {
    use CodingAgentClaimKind as K;
    use CodingAgentGateDecision as D;
    let contexts = load_context();
    let mut doc = load_doc(&fixtures().join("vectors/MV-03.json"));
    let ctx = context_for(&contexts, "MV-03");
    assert_eq!(
        cap1_claim_decision(&doc, K::BoundedNegative, &ctx).decision,
        D::Blocked
    );
    assert_eq!(
        cap1_claim_decision(&doc, K::ExhaustiveSet, &ctx).decision,
        D::Degraded
    );
    for support in [None, Some(Vec::new())] {
        doc.absence_assertions = support;
        for (kind, expected) in [
            (K::BoundedNegative, D::Blocked),
            (K::ExhaustiveSet, D::Degraded),
        ] {
            let result = cap1_claim_decision(&doc, kind, &ctx);
            assert_eq!(
                result.decision, expected,
                "missing cited support, {kind:?}, {:?}",
                doc.absence_assertions
            );
            assert_eq!(
                result.rules_fired(),
                vec![Cap1Rule::AbsenceBoundedByExaminedUnits]
            );
            assert_eq!(
                serde_json::to_value(result.findings[0].gap).unwrap(),
                "claim_support_missing"
            );
        }
        assert_eq!(
            cap1_claim_decision(&doc, K::PositiveExistence, &ctx).decision,
            D::Allowed
        );
    }
    let honest = load_doc(&fixtures().join("vectors/HV-03.json"));
    assert_eq!(
        cap1_claim_decision(
            &honest,
            K::BoundedNegative,
            &context_for(&contexts, "HV-03")
        )
        .decision,
        D::Allowed
    );
}
