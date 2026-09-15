//! CAP-1 normative rules R0–R8 (`draft-hillier-coverage-attestation-00` §5), evaluated over
//! the typed document in numeric order, across all strata, stopping at the first failure.
//!
//! Every rule is stated in full from the draft text even where the pinned schema or the typed
//! shape already enforces part of it, so that the rule stage has one reading of each rule and
//! does not depend on which earlier stage happened to run. The rules that the schema fully
//! covers (closed disposition vocabulary, closed basis kinds, non-negative integer counts) are
//! carried by the typed enums and unsigned counts and are noted at their rule.

use std::collections::BTreeSet;

use super::verify::Cap1Refusal;
use super::{Cap1BasisKind, Cap1Disposition, Cap1Document};

/// The nine normative rules, named with the upstream rule ids so run records line up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Cap1NormativeRule {
    R0Shape,
    R1NoSilentRemainder,
    R2ClosedDisposition,
    R3WithholdingDigestBound,
    R4DenominatorBasis,
    R5CountsWellFormed,
    R6AbsenceIsScoped,
    R7IncompleteNotClean,
    R8SupportsBoundsCitation,
}

impl Cap1NormativeRule {
    pub const ALL: [Cap1NormativeRule; 9] = [
        Self::R0Shape,
        Self::R1NoSilentRemainder,
        Self::R2ClosedDisposition,
        Self::R3WithholdingDigestBound,
        Self::R4DenominatorBasis,
        Self::R5CountsWellFormed,
        Self::R6AbsenceIsScoped,
        Self::R7IncompleteNotClean,
        Self::R8SupportsBoundsCitation,
    ];

    /// The upstream rule id as the reference verifiers spell it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::R0Shape => "R0-shape",
            Self::R1NoSilentRemainder => "R1-no-silent-remainder",
            Self::R2ClosedDisposition => "R2-closed-disposition",
            Self::R3WithholdingDigestBound => "R3-withholding-digest-bound",
            Self::R4DenominatorBasis => "R4-denominator-basis",
            Self::R5CountsWellFormed => "R5-counts-well-formed",
            Self::R6AbsenceIsScoped => "R6-absence-is-scoped",
            Self::R7IncompleteNotClean => "R7-incomplete-not-clean",
            Self::R8SupportsBoundsCitation => "R8-supports-bounds-citation",
        }
    }
}

/// The schema's digest pattern, `^[0-9a-f]{32,128}$`, read here for R3 and R4 because those
/// rules require the digest to be present, which the schema does not.
fn is_hex_digest(s: &str) -> bool {
    (32..=128).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Dispositions that mean a unit was dispatched, or should have been, and produced no result.
/// R7 turns on this set; `out_of_scope` and `disabled_by_policy` are accounted decisions.
fn is_hard_stop(d: Cap1Disposition) -> bool {
    matches!(
        d,
        Cap1Disposition::Failed | Cap1Disposition::ResourceExhausted | Cap1Disposition::Unavailable
    )
}

fn refuse(rule: Cap1NormativeRule, grounds: &'static str, at: Option<String>) -> Cap1Refusal {
    Cap1Refusal::Rule { rule, grounds, at }
}

fn stratum_at(i: usize) -> Option<String> {
    Some(format!("/strata/{i}"))
}

fn unit_at(i: usize, j: usize) -> Option<String> {
    Some(format!("/strata/{i}/unexamined/{j}"))
}

/// R0–R8 in numeric order over a typed document; the first failure is returned.
pub fn verify_cap1_rules(doc: &Cap1Document) -> Result<(), Cap1Refusal> {
    verify_cap1_rules_with(doc, &[])
}

/// As [`verify_cap1_rules`], with named rules silenced. Exists for mutation testing: a rule
/// whose silence changes no outcome is decoration.
pub fn verify_cap1_rules_with(
    doc: &Cap1Document,
    disabled: &[Cap1NormativeRule],
) -> Result<(), Cap1Refusal> {
    use Cap1NormativeRule as R;
    let on = |r: R| !disabled.contains(&r);

    // R0, shape. Object, profile cap/1, a subject, at least one stratum and a boolean
    // integrity.complete are the typed shape; stratum ids present and unique are checked here.
    if on(R::R0Shape) {
        if doc.profile != "cap/1" {
            return Err(refuse(R::R0Shape, "profile is not cap/1", None));
        }
        if doc.strata.is_empty() {
            return Err(refuse(R::R0Shape, "no strata", None));
        }
        let mut ids = BTreeSet::new();
        for (i, s) in doc.strata.iter().enumerate() {
            if s.id.is_empty() {
                return Err(refuse(R::R0Shape, "stratum id absent", stratum_at(i)));
            }
            if !ids.insert(s.id.as_str()) {
                return Err(refuse(R::R0Shape, "stratum id duplicated", stratum_at(i)));
            }
        }
    }

    // R1, no silent remainder: eligible == examined + accounted unexamined units.
    if on(R::R1NoSilentRemainder) {
        for (i, s) in doc.strata.iter().enumerate() {
            let accounted = u64::try_from(s.unexamined.len()).ok();
            let reconciled = accounted
                .and_then(|n| s.examined.checked_add(n))
                .is_some_and(|sum| sum == s.eligible);
            if !reconciled {
                return Err(refuse(
                    R::R1NoSilentRemainder,
                    "eligible does not equal examined plus accounted unexamined",
                    stratum_at(i),
                ));
            }
        }
    }

    // R2, closed disposition: every entry names a unit; the vocabulary is the typed enum.
    if on(R::R2ClosedDisposition) {
        for (i, s) in doc.strata.iter().enumerate() {
            for (j, u) in s.unexamined.iter().enumerate() {
                if u.unit.is_empty() {
                    return Err(refuse(
                        R::R2ClosedDisposition,
                        "unexamined entry names no unit",
                        unit_at(i, j),
                    ));
                }
            }
        }
    }

    // R3, withholding is digest-bound.
    if on(R::R3WithholdingDigestBound) {
        for (i, s) in doc.strata.iter().enumerate() {
            for (j, u) in s.unexamined.iter().enumerate() {
                if u.disposition == Cap1Disposition::Withheld
                    && !u.withheld_digest.as_deref().is_some_and(is_hex_digest)
                {
                    return Err(refuse(
                        R::R3WithholdingDigestBound,
                        "withheld unit carries no digest of the withheld value",
                        unit_at(i, j),
                    ));
                }
            }
        }
    }

    // R4, denominator basis: kind is the typed enum; catalogue needs a digest, enumeration a
    // stated method.
    if on(R::R4DenominatorBasis) {
        for (i, s) in doc.strata.iter().enumerate() {
            let ok = match s.basis.kind {
                Cap1BasisKind::Catalogue => s
                    .basis
                    .catalogue_digest
                    .as_deref()
                    .is_some_and(is_hex_digest),
                Cap1BasisKind::Enumeration => s
                    .basis
                    .enumeration_method
                    .as_deref()
                    .is_some_and(|m| !m.is_empty()),
                Cap1BasisKind::Declared => true,
            };
            if !ok {
                return Err(refuse(
                    R::R4DenominatorBasis,
                    "basis lacks the digest or method its kind requires",
                    stratum_at(i),
                ));
            }
        }
    }

    // R5, counts well formed: non-negative integers are the typed shape; examined <= eligible.
    if on(R::R5CountsWellFormed) {
        for (i, s) in doc.strata.iter().enumerate() {
            if s.examined > s.eligible {
                return Err(refuse(
                    R::R5CountsWellFormed,
                    "examined exceeds eligible",
                    stratum_at(i),
                ));
            }
        }
    }

    let ids: BTreeSet<&str> = doc.strata.iter().map(|s| s.id.as_str()).collect();
    let assertions = doc.absence_assertions.as_deref().unwrap_or(&[]);

    // R6, absence is scoped: every assertion names an existing stratum.
    if on(R::R6AbsenceIsScoped) {
        for (k, a) in assertions.iter().enumerate() {
            if a.stratum.is_empty() || !ids.contains(a.stratum.as_str()) {
                return Err(refuse(
                    R::R6AbsenceIsScoped,
                    "absence assertion names no stratum, or an unknown one",
                    Some(format!("/absence_assertions/{k}")),
                ));
            }
        }
    }

    // R7, incomplete is not clean.
    if on(R::R7IncompleteNotClean) {
        let any_hard_stop = doc
            .strata
            .iter()
            .flat_map(|s| s.unexamined.iter())
            .any(|u| is_hard_stop(u.disposition));
        if any_hard_stop && doc.integrity.complete {
            return Err(refuse(
                R::R7IncompleteNotClean,
                "integrity.complete is true while units failed, were exhausted or unavailable",
                Some("/integrity/complete".to_string()),
            ));
        }
        let capped = doc
            .integrity
            .capped_to
            .as_deref()
            .is_some_and(|c| !c.is_empty());
        if !doc.integrity.complete && !capped {
            return Err(refuse(
                R::R7IncompleteNotClean,
                "integrity.complete is false and no capped_to verdict is stated",
                Some("/integrity/capped_to".to_string()),
            ));
        }
    }

    // R8, supports bounds citation: a cited stratum states its supported claim classes.
    if on(R::R8SupportsBoundsCitation) {
        let cited: BTreeSet<&str> = assertions.iter().map(|a| a.stratum.as_str()).collect();
        for (i, s) in doc.strata.iter().enumerate() {
            let states_supports = s.supports.as_deref().is_some_and(|v| !v.is_empty());
            if cited.contains(s.id.as_str()) && !states_supports {
                return Err(refuse(
                    R::R8SupportsBoundsCitation,
                    "stratum is cited by an absence assertion but states no supported claim classes",
                    stratum_at(i),
                ));
            }
        }
    }

    Ok(())
}
