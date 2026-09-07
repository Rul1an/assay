//! Relying-party decision layer over a CAP-1 coverage attestation.
//!
//! CAP-1, `draft-hillier-coverage-attestation-00` (Hillier / Certisyn, 2026-08-20), is a
//! tool-agnostic vocabulary for stating what an examination examined, what it did not, and why:
//! a declared denominator with a declared basis, a closed disposition for every unexamined unit,
//! and normative rules R0 to R8 that a verifier enforces. Its sections 1.3 and 9 say exactly what
//! conformance establishes: internal consistency, never truthfulness, and nothing about the
//! producer. "A conforming document may be entirely false."
//!
//! This module is the layer those sections scope out. Given a document that already CONFORMS
//! (this module runs no CAP-1 verifier and claims no CAP-1 implementation) and the relying
//! party's OWN state, it answers, per claim kind, what the accounting supports. The vocabulary is
//! the one this crate already uses for coding-agent evidence and the runner substrate mirrors:
//! [`CodingAgentClaimKind`] in, [`CodingAgentGateDecision`] out, worst wins.
//!
//! One rule, one function. The claim-kind asymmetry (partial coverage supports "this happened",
//! degrades "these are all of them", blocks "this did not happen"; a self-reported account caps
//! at `asserted`) is the runner substrate's rule, restated here over a CAP-1 stratum rather than
//! reinvented. `tests/cap1_relying_party_gate.rs` pins agreement with
//! [`coding_agent_claim_decision`] on the two shapes that overlap, and pins decision parity with
//! the Python reference at `github.com/Rul1an/cap1-conforming-but-misleading`, whose vectors
//! are the fixtures.
//!
//! What the rules read, stated exactly. Every rule derives from the document plus the relying
//! party's context. The document's identities (producer name, subject ref, unit names, catalogue
//! digest, counts) are producer-written strings and this gate authenticates none of them: a
//! producer who relabels an identity to one the relying party accepts is not caught here.
//! Binding identities is a composition profile's job; Iman Schrock's
//! `EP-CAP1-COVERAGE-COMPOSITION-v1` (emilia-protocol, 2026-08-22) is the published instance and
//! already binds five of these seven axes at the byte layer. Absence is evaluated over the stratum
//! the assertion cites, which is what R6 binds it to; CAP-1's free-text `qualifier` is not read.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::coding_agent::{CodingAgentClaimKind, CodingAgentGateDecision};

/// A CAP-1 document, as the normative JSON Schema shapes it (`additionalProperties: false`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cap1Document {
    pub profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer: Option<Cap1Producer>,
    pub subject: Cap1Subject,
    pub strata: Vec<Cap1Stratum>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub absence_assertions: Option<Vec<Cap1AbsenceAssertion>>,
    pub integrity: Cap1Integrity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_of: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cap1Producer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cap1Subject {
    pub kind: String,
    #[serde(rename = "ref")]
    pub reference: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<Cap1Digest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cap1Digest {
    pub algorithm: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cap1Stratum {
    pub id: String,
    pub population: String,
    pub basis: Cap1Basis,
    pub eligible: u64,
    pub examined: u64,
    pub unexamined: Vec<Cap1Unexamined>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cap1Basis {
    pub kind: Cap1BasisKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalogue_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalogue_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enumeration_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// CAP-1 section 4.2: ordered by how much a reader can do with them, weakest last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cap1BasisKind {
    Catalogue,
    Enumeration,
    Declared,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cap1Unexamined {
    pub unit: String,
    pub disposition: Cap1Disposition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub withheld_digest: Option<String>,
}

/// CAP-1 section 6, the closed vocabulary. Closed here too: an unknown value fails to parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cap1Disposition {
    NotApplicable,
    DisabledByPolicy,
    UnsupportedInput,
    ResourceExhausted,
    Failed,
    Unavailable,
    OutOfScope,
    Withheld,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cap1AbsenceAssertion {
    pub assertion: String,
    pub stratum: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qualifier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cap1Integrity {
    pub complete: bool,
    pub statement: String,
    #[serde(default)]
    pub uncapped_verdict: Option<String>,
    #[serde(default)]
    pub capped_to: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unaccounted: Option<Vec<String>>,
}

/// The relying party's OWN state. Nothing in it is taken from a producer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cap1RelyingPartyContext {
    /// Catalogue digests the relying party fixed BEFORE any result was known.
    #[serde(default)]
    pub precommitted_catalogue_digests: Vec<String>,
    /// Catalogue digest to the unit list the relying party itself holds behind it.
    #[serde(default)]
    pub catalogue_units: BTreeMap<String, Vec<String>>,
    /// Units the relying party can itself show do not apply to the subject.
    #[serde(default)]
    pub confirmable_not_applicable: Vec<String>,
    /// Units the relying party's own authorisation excludes.
    #[serde(default)]
    pub confirmable_out_of_scope: Vec<String>,
    /// Withheld digest to the result bytes the relying party holds; checked by hash.
    #[serde(default)]
    pub withheld_results: BTreeMap<String, String>,
    /// Producer names the relying party accepts as observers distinct from the subject.
    /// Names, not authenticated identities.
    #[serde(default)]
    pub independent_producers: Vec<String>,
}

/// The seven rules, named as the Python reference names them so run records line up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Cap1Rule {
    #[serde(rename = "C1_denominator_precommitted")]
    DenominatorPrecommitted,
    #[serde(rename = "C2_applicability_confirmable")]
    ApplicabilityConfirmable,
    #[serde(rename = "C3_absence_bounded_by_examined_units")]
    AbsenceBoundedByExaminedUnits,
    #[serde(rename = "C4_withheld_resolvable")]
    WithheldResolvable,
    #[serde(rename = "C5_population_granularity")]
    PopulationGranularity,
    #[serde(rename = "C6_producer_independent_of_subject")]
    ProducerIndependentOfSubject,
    #[serde(rename = "C7_unit_identity_unique")]
    UnitIdentityUnique,
}

impl Cap1Rule {
    pub const ALL: [Cap1Rule; 7] = [
        Cap1Rule::DenominatorPrecommitted,
        Cap1Rule::ApplicabilityConfirmable,
        Cap1Rule::AbsenceBoundedByExaminedUnits,
        Cap1Rule::WithheldResolvable,
        Cap1Rule::PopulationGranularity,
        Cap1Rule::ProducerIndependentOfSubject,
        Cap1Rule::UnitIdentityUnique,
    ];

    /// The Python reference's rule id, for run-record parity.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Cap1Rule::DenominatorPrecommitted => "C1_denominator_precommitted",
            Cap1Rule::ApplicabilityConfirmable => "C2_applicability_confirmable",
            Cap1Rule::AbsenceBoundedByExaminedUnits => "C3_absence_bounded_by_examined_units",
            Cap1Rule::WithheldResolvable => "C4_withheld_resolvable",
            Cap1Rule::PopulationGranularity => "C5_population_granularity",
            Cap1Rule::ProducerIndependentOfSubject => "C6_producer_independent_of_subject",
            Cap1Rule::UnitIdentityUnique => "C7_unit_identity_unique",
        }
    }
}

/// What is missing, when something is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cap1Gap {
    DenominatorPostHoc,
    DenominatorNotPrecommitted,
    SelfClassifiedDisposition,
    PartialOnly,
    WithheldUnresolvable,
    PopulationMismatch,
    SelfReportedOnly,
    ProducerNotAcceptedIndependent,
    DuplicateUnitIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cap1Finding {
    pub rule: Cap1Rule,
    pub decision: CodingAgentGateDecision,
    pub gap: Cap1Gap,
    pub detail: String,
}

/// What a relying party may conclude from one CAP-1 document, for one kind of claim.
///
/// No claim ceiling is assigned: the ceiling ladder needs a source class, and a CAP-1 document
/// carries a producer name, not a vantage. A `SelfReportedOnly` gap says the one thing this
/// gate can know about vantage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cap1ClaimDecision {
    pub claim_kind: CodingAgentClaimKind,
    pub decision: CodingAgentGateDecision,
    pub findings: Vec<Cap1Finding>,
}

impl Cap1ClaimDecision {
    /// The rules that fired, sorted, as the Python run record lists them.
    #[must_use]
    pub fn rules_fired(&self) -> Vec<Cap1Rule> {
        let set: BTreeSet<Cap1Rule> = self.findings.iter().map(|f| f.rule).collect();
        set.into_iter().collect()
    }
}

fn worst(a: CodingAgentGateDecision, b: CodingAgentGateDecision) -> CodingAgentGateDecision {
    use CodingAgentGateDecision as D;
    match (a, b) {
        (D::Blocked, _) | (_, D::Blocked) => D::Blocked,
        (D::Degraded, _) | (_, D::Degraded) => D::Degraded,
        _ => D::Allowed,
    }
}

/// Evaluate a conforming CAP-1 document for one claim kind against the relying party's state.
#[must_use]
pub fn cap1_claim_decision(
    doc: &Cap1Document,
    claim_kind: CodingAgentClaimKind,
    ctx: &Cap1RelyingPartyContext,
) -> Cap1ClaimDecision {
    cap1_claim_decision_with(doc, claim_kind, ctx, &[])
}

/// As [`cap1_claim_decision`], with named rules silenced. Exists for mutation testing: a rule
/// whose silence changes no decision is decoration.
#[must_use]
pub fn cap1_claim_decision_with(
    doc: &Cap1Document,
    claim_kind: CodingAgentClaimKind,
    ctx: &Cap1RelyingPartyContext,
    disabled: &[Cap1Rule],
) -> Cap1ClaimDecision {
    use CodingAgentClaimKind as Kind;
    use CodingAgentGateDecision as D;

    let on = |r: Cap1Rule| !disabled.contains(&r);
    let neg = claim_kind != Kind::PositiveExistence;
    let mut findings: Vec<Cap1Finding> = Vec::new();
    let mut add = |rule: Cap1Rule, decision: D, gap: Cap1Gap, detail: String| {
        findings.push(Cap1Finding {
            rule,
            decision,
            gap,
            detail,
        });
    };

    let cited: BTreeSet<&str> = doc
        .absence_assertions
        .iter()
        .flatten()
        .map(|a| a.stratum.as_str())
        .collect();
    let producer = doc.producer.as_ref().and_then(|p| p.name.as_deref());
    let subject_ref = doc.subject.reference.as_str();

    // C6, document-wide: who produced the accounting. Two branches, each with its own vector.
    // A producer that impersonates an accepted name passes this string check; that is the
    // authentication gap named in the module docs.
    if on(Cap1Rule::ProducerIndependentOfSubject) {
        let accepted = producer.is_some_and(|p| ctx.independent_producers.iter().any(|n| n == p));
        if producer == Some(subject_ref) {
            add(
                Cap1Rule::ProducerIndependentOfSubject,
                if neg { D::Blocked } else { D::Degraded },
                Cap1Gap::SelfReportedOnly,
                format!(
                    "producer {} is the subject; coverage is self-reported, ceiling asserted",
                    subject_ref
                ),
            );
        } else if neg && !accepted {
            add(
                Cap1Rule::ProducerIndependentOfSubject,
                D::Degraded,
                Cap1Gap::ProducerNotAcceptedIndependent,
                format!(
                    "producer {:?} is not one the relying party accepts as independent of the subject",
                    producer.unwrap_or("")
                ),
            );
        }
    }

    for s in &doc.strata {
        if !cited.contains(s.id.as_str()) {
            continue; // only strata offered in support of an absence claim are gated
        }
        let sid = s.id.as_str();

        // C7: unit identity must be unique within the accounting. CAP-1 does not forbid a
        // duplicate unit name, so an errored unit can hide under the name of a confirmed one.
        // First named and refused by Iman Schrock (INTERPRETATION.md, 2026-08-22); Anton
        // Sokolov's 25 Aug item is the same accounting-layer point. Konrad Gruszka's F6 is the
        // parse layer (duplicate JSON member names) and a different finding.
        if on(Cap1Rule::UnitIdentityUnique) && neg {
            let mut seen = BTreeSet::new();
            let mut dups = BTreeSet::new();
            for u in &s.unexamined {
                if !seen.insert(u.unit.as_str()) {
                    dups.insert(u.unit.as_str());
                }
            }
            if !dups.is_empty() {
                add(
                    Cap1Rule::UnitIdentityUnique,
                    D::Blocked,
                    Cap1Gap::DuplicateUnitIdentity,
                    format!("strata[{sid}] unit names appear more than once in the accounting: {dups:?}"),
                );
            }
        }

        // C1: the denominator must have been fixed before results were known, and the relying
        // party must be able to say so from its own record. `declared` never satisfies this;
        // `catalogue` does for a precommitted digest; `enumeration` never does, because the
        // relying party cannot re-run the producer's method.
        if on(Cap1Rule::DenominatorPrecommitted) && neg {
            match s.basis.kind {
                Cap1BasisKind::Declared => add(
                    Cap1Rule::DenominatorPrecommitted,
                    D::Blocked,
                    Cap1Gap::DenominatorPostHoc,
                    format!("strata[{sid}] basis is declared; the population can have been fixed after results"),
                ),
                Cap1BasisKind::Catalogue => {
                    let pre = s
                        .basis
                        .catalogue_digest
                        .as_deref()
                        .is_some_and(|d| ctx.precommitted_catalogue_digests.iter().any(|p| p == d));
                    if !pre {
                        add(
                            Cap1Rule::DenominatorPrecommitted,
                            D::Degraded,
                            Cap1Gap::DenominatorNotPrecommitted,
                            format!("strata[{sid}] catalogue digest is not one the relying party precommitted to"),
                        );
                    }
                }
                Cap1BasisKind::Enumeration => add(
                    Cap1Rule::DenominatorPrecommitted,
                    D::Degraded,
                    Cap1Gap::DenominatorNotPrecommitted,
                    format!(
                        "strata[{sid}] enumerated by the producer ({:?}); the relying party cannot re-run it",
                        s.basis.enumeration_method.as_deref().unwrap_or("")
                    ),
                ),
            }
        }

        // C2: an applicability or authorisation disposition is the producer's classification of
        // a unit, and R7 never fires on it. Each one the relying party cannot confirm from its
        // own knowledge of the subject (not_applicable) or its own authorisation (out_of_scope)
        // degrades an absence claim over the stratum.
        if on(Cap1Rule::ApplicabilityConfirmable) && neg {
            let unconfirmed: Vec<&str> = s
                .unexamined
                .iter()
                .filter(|u| match u.disposition {
                    Cap1Disposition::NotApplicable => {
                        !ctx.confirmable_not_applicable.contains(&u.unit)
                    }
                    Cap1Disposition::OutOfScope => !ctx.confirmable_out_of_scope.contains(&u.unit),
                    _ => false,
                })
                .map(|u| u.unit.as_str())
                .collect();
            if !unconfirmed.is_empty() {
                add(
                    Cap1Rule::ApplicabilityConfirmable,
                    D::Degraded,
                    Cap1Gap::SelfClassifiedDisposition,
                    format!("strata[{sid}] applicability/authorisation dispositions the relying party cannot confirm: {unconfirmed:?}"),
                );
            }
        }

        // C3: absence over the stratum is bounded by the units that could have examined the
        // input. Any such unit that did not means absence is not a fact about it. All or
        // nothing, not a share. The runner gate's asymmetry: degrades "these are all of them",
        // blocks "this did not happen".
        if on(Cap1Rule::AbsenceBoundedByExaminedUnits) && neg {
            let shortfall: Vec<&str> = s
                .unexamined
                .iter()
                .filter(|u| {
                    !matches!(
                        u.disposition,
                        Cap1Disposition::NotApplicable
                            | Cap1Disposition::OutOfScope
                            | Cap1Disposition::Withheld
                    )
                })
                .map(|u| u.unit.as_str())
                .collect();
            if !shortfall.is_empty() {
                add(
                    Cap1Rule::AbsenceBoundedByExaminedUnits,
                    if claim_kind == Kind::BoundedNegative {
                        D::Blocked
                    } else {
                        D::Degraded
                    },
                    Cap1Gap::PartialOnly,
                    format!(
                        "strata[{sid}] examined {}/{}; absence over the stratum is not a fact about units that did not examine the input: {shortfall:?}",
                        s.examined, s.eligible
                    ),
                );
            }
        }

        // C4: withheld means examined-but-undisclosed. It supports absence for the relying party
        // only if the relying party holds result bytes that hash to the withheld digest.
        if on(Cap1Rule::WithheldResolvable) && neg {
            let unresolved: Vec<&str> = s
                .unexamined
                .iter()
                .filter(|u| u.disposition == Cap1Disposition::Withheld)
                .filter(|u| {
                    let Some(d) = u.withheld_digest.as_deref() else {
                        return true;
                    };
                    match ctx.withheld_results.get(d) {
                        Some(bytes) => hex::encode(Sha256::digest(bytes.as_bytes())) != d,
                        None => true,
                    }
                })
                .map(|u| u.unit.as_str())
                .collect();
            if !unresolved.is_empty() {
                add(
                    Cap1Rule::WithheldResolvable,
                    D::Degraded,
                    Cap1Gap::WithheldUnresolvable,
                    format!("strata[{sid}] withheld results the relying party cannot resolve: {unresolved:?}"),
                );
            }
        }

        // C5: the population must be the relying party's own catalogue, not merely carry its
        // digest. A one-unit count, a dropped unit, or a stratum split all agree with the digest
        // and disagree with the set. This is Iman Schrock's verifier-owned population pin
        // (EP-CAP1-COVERAGE-COMPOSITION-v1) restated as a decision.
        if on(Cap1Rule::PopulationGranularity) && neg && s.basis.kind == Cap1BasisKind::Catalogue {
            if let Some(units) = s
                .basis
                .catalogue_digest
                .as_deref()
                .and_then(|d| ctx.catalogue_units.get(d))
            {
                let set: BTreeSet<&str> = units.iter().map(String::as_str).collect();
                let foreign: Vec<&str> = s
                    .unexamined
                    .iter()
                    .map(|u| u.unit.as_str())
                    .filter(|u| !set.contains(u))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                if s.eligible != set.len() as u64 || !foreign.is_empty() {
                    add(
                        Cap1Rule::PopulationGranularity,
                        D::Blocked,
                        Cap1Gap::PopulationMismatch,
                        format!(
                            "strata[{sid}] eligible {} vs {} units in the relying party's catalogue; units outside it: {foreign:?}",
                            s.eligible,
                            set.len()
                        ),
                    );
                }
            }
        }
    }

    let decision = findings
        .iter()
        .fold(D::Allowed, |acc, f| worst(acc, f.decision));
    Cap1ClaimDecision {
        claim_kind,
        decision,
        findings,
    }
}
