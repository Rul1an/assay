//! One behaviour class per test. Nothing here is composite: a test that walks a table proves only
//! its first failing row, so each row is its own test and each bites on its own assertion.

use super::*;

const V2026: &str = RESULT_TYPE_SINCE;
const V2025: &str = "2025-06-18";

// --- Era resolution ----------------------------------------------------------------------------

#[test]
fn no_signal_anywhere_is_unknown_no_signal() {
    assert_eq!(
        resolve_era(
            &EnvelopeObservation::NotApplicable,
            &RequestMetadata::Absent
        ),
        EraResolution::Unknown(UnknownReason::NoSignal)
    );
}

/// The correction that matters most: an unframed format with body metadata has a known era. An
/// umbrella refusal on `NotApplicable` would ceiling valid evidence for a signal the format was
/// never required to carry.
#[test]
fn body_metadata_alone_resolves_the_era() {
    assert_eq!(
        resolve_era(
            &EnvelopeObservation::NotApplicable,
            &RequestMetadata::Present(V2026.into())
        ),
        EraResolution::Known(V2026.into())
    );
}

#[test]
fn a_header_alone_resolves_the_era() {
    assert_eq!(
        resolve_era(
            &EnvelopeObservation::Present(V2025.into()),
            &RequestMetadata::Absent
        ),
        EraResolution::Known(V2025.into())
    );
}

#[test]
fn agreeing_signals_resolve_to_known() {
    assert_eq!(
        resolve_era(
            &EnvelopeObservation::Present(V2026.into()),
            &RequestMetadata::Present(V2026.into())
        ),
        EraResolution::Known(V2026.into())
    );
}

/// The spec's own rule, not one this design invented.
#[test]
fn disagreeing_signals_resolve_to_conflicting() {
    assert_eq!(
        resolve_era(
            &EnvelopeObservation::Present(V2025.into()),
            &RequestMetadata::Present(V2026.into())
        ),
        EraResolution::Conflicting {
            header: V2025.into(),
            body: V2026.into()
        }
    );
}

/// A well-formed date this build has no rules for. Distinct from silence: the signal arrived and
/// was readable, and the gap is in the reader.
#[test]
fn a_wellformed_but_unsupported_version_is_unsupported_not_missing() {
    assert_eq!(
        resolve_era(
            &EnvelopeObservation::Present("2031-01-01".into()),
            &RequestMetadata::Absent
        ),
        EraResolution::Unknown(UnknownReason::UnsupportedVersion("2031-01-01".into()))
    );
}

/// Lexicographic comparison against `RESULT_TYPE_SINCE` is only sound once the value is known to
/// be a version at all, so an unusable one must be caught before any ordering is attempted.
#[test]
fn a_malformed_signal_is_malformed_not_unsupported() {
    assert_eq!(
        resolve_era(
            &EnvelopeObservation::NotApplicable,
            &RequestMetadata::Malformed
        ),
        EraResolution::Unknown(UnknownReason::MalformedSignal)
    );
}

// --- Reading a response under the era ----------------------------------------------------------

/// The positive case. Without it an implementation that refuses every modern result passes the
/// rest of this file.
#[test]
fn complete_at_2026_is_terminal() {
    assert_eq!(
        conclude(
            &EraResolution::Known(V2026.into()),
            &ResultObservation::Complete,
            None
        ),
        ResultConclusion::Terminal
    );
}

/// Valid, and not finished. The distinction a boolean cannot carry.
#[test]
fn input_required_is_valid_but_not_terminal() {
    assert_eq!(
        conclude(
            &EraResolution::Known(V2026.into()),
            &ResultObservation::InputRequired,
            None
        ),
        ResultConclusion::NonTerminal
    );
}

/// The closed answer, which is now the capability-relative one. `CoreOnly` is what this assertion
/// always meant: a set was stated and nothing in it could cover the token. Passing no observation
/// here would assert a different thing, because an unseen set cannot settle the question.
#[test]
fn an_unrecognized_token_is_incomplete_not_invalid() {
    assert_eq!(
        conclude(
            &EraResolution::Known(V2026.into()),
            &ResultObservation::Unrecognized,
            Some(&CapabilityObservation::CoreOnly)
        ),
        ResultConclusion::Incomplete(IncompleteReason::UnrecognizedResultType)
    );
}

#[test]
fn missing_result_type_at_2026_is_invalid() {
    assert_eq!(
        conclude(
            &EraResolution::Known(V2026.into()),
            &ResultObservation::Missing,
            None
        ),
        ResultConclusion::Invalid(InvalidReason::MissingResultType)
    );
}

/// The other arm of the same pair, as its own test so it is reached even when the first fails.
#[test]
fn missing_result_type_before_2026_is_terminal() {
    assert_eq!(
        conclude(
            &EraResolution::Known(V2025.into()),
            &ResultObservation::Missing,
            None
        ),
        ResultConclusion::Terminal
    );
}

#[test]
fn an_unknown_era_blocks_as_incomplete() {
    assert_eq!(
        conclude(
            &EraResolution::Unknown(UnknownReason::NoSignal),
            &ResultObservation::Complete,
            None
        ),
        ResultConclusion::Incomplete(IncompleteReason::EraUnknown(UnknownReason::NoSignal))
    );
}

/// Contradiction is not silence: unknown may become conclusive with more evidence, this will not.
#[test]
fn a_conflicting_era_blocks_as_invalid() {
    assert_eq!(
        conclude(
            &EraResolution::Conflicting {
                header: V2025.into(),
                body: V2026.into()
            },
            &ResultObservation::Complete,
            None
        ),
        ResultConclusion::Invalid(InvalidReason::EraConflicting {
            header: V2025.into(),
            body: V2026.into()
        })
    );
}

// --- Reading a request under the era -----------------------------------------------------------

/// `RequestParams._meta` and the version inside it are both required at 2026, with no `?`, so a
/// request that resolves to that era and carries neither is a fault on its own terms.
#[test]
fn a_2026_request_without_metadata_is_invalid() {
    assert_eq!(
        conclude_request(
            &EraResolution::Known(V2026.into()),
            &RequestMetadata::Absent,
            Some(&CapabilityObservation::CoreOnly),
        ),
        RequestAssessment::Invalid(InvalidReason::MissingRequestMetadata)
    );
}

/// Missing and unreadable are different findings, which is why the observation is typed.
#[test]
fn a_2026_request_with_malformed_metadata_is_invalid_for_a_different_reason() {
    assert_eq!(
        conclude_request(
            &EraResolution::Known(V2026.into()),
            &RequestMetadata::Malformed,
            Some(&CapabilityObservation::CoreOnly),
        ),
        RequestAssessment::Invalid(InvalidReason::MalformedRequestMetadata)
    );
}

/// Before the field existed its absence is not a fault, so the rule must not become blanket.
#[test]
fn a_legacy_request_without_metadata_is_not_invalid() {
    assert_eq!(
        conclude_request(
            &EraResolution::Known(V2025.into()),
            &RequestMetadata::Absent,
            Some(&CapabilityObservation::CoreOnly),
        ),
        RequestAssessment::Valid
    );
}

#[test]
fn a_2026_request_carrying_metadata_is_not_invalid() {
    assert_eq!(
        conclude_request(
            &EraResolution::Known(V2026.into()),
            &RequestMetadata::Present(V2026.into()),
            Some(&CapabilityObservation::CoreOnly),
        ),
        RequestAssessment::Valid
    );
}

/// A signal that arrived and is not a version. Found by mutation: with only the `Malformed`
/// observation covered, deleting the shape check left every test green, because that path reaches
/// `MalformedSignal` through the observation rather than through the check. Lexicographic
/// comparison against `RESULT_TYPE_SINCE` is date comparison only once the value is known to be a
/// date, so the check has to be the thing under test.
#[test]
fn a_present_signal_that_is_not_a_version_is_malformed() {
    assert_eq!(
        resolve_era(
            &EnvelopeObservation::Present("not-a-date".into()),
            &RequestMetadata::Absent
        ),
        EraResolution::Unknown(UnknownReason::MalformedSignal)
    );
}

/// The trap the shape check exists for: a string that sorts above `RESULT_TYPE_SINCE` without
/// being a date at all. Unguarded it would classify as a version and compare successfully.
#[test]
fn a_non_version_that_sorts_high_is_still_malformed() {
    assert_eq!(
        resolve_era(
            &EnvelopeObservation::NotApplicable,
            &RequestMetadata::Present("9999-not-a-date".into())
        ),
        EraResolution::Unknown(UnknownReason::MalformedSignal)
    );
}

// --- Composite: the chain a caller actually walks ----------------------------------------------
//
// The tests above call `conclude_request` with an era handed to it directly. That can assert on a
// pair the resolver never produces, which is how a unit-green implementation fails open in use.
// These walk resolve-then-conclude, and they are the ones that decide.

/// A 2026 header with unreadable metadata. `resolve_era` cannot return `Known` here, so the
/// request-side fault has to survive an unresolved era or it is lost. Malformed metadata is a
/// fault whenever it appears: unlike absence, it does not depend on which version is in force.
#[test]
fn composite_malformed_metadata_under_a_2026_header_is_invalid() {
    let envelope = EnvelopeObservation::Present(V2026.into());
    let metadata = RequestMetadata::Malformed;
    let era = resolve_era(&envelope, &metadata);
    assert_eq!(
        conclude_request(&era, &metadata, Some(&CapabilityObservation::CoreOnly)),
        RequestAssessment::Invalid(InvalidReason::MalformedRequestMetadata)
    );
}

/// A contradicted request with no response at all. Not double counting is the aggregator's job:
/// a refused or aborted request is exactly the case that never produces a response, so leaving the
/// fault to the response side loses it entirely.
#[test]
fn composite_a_conflicting_request_without_a_response_is_invalid() {
    let envelope = EnvelopeObservation::Present(V2025.into());
    let metadata = RequestMetadata::Present(V2026.into());
    let era = resolve_era(&envelope, &metadata);
    assert_eq!(
        conclude_request(&era, &metadata, Some(&CapabilityObservation::CoreOnly)),
        RequestAssessment::Invalid(InvalidReason::EraConflicting {
            header: V2025.into(),
            body: V2026.into()
        })
    );
}

/// Unreadable beats contradicted. Two values that are not both versions cannot be said to
/// disagree about a version, so shape has to be settled before any comparison.
#[test]
fn composite_a_malformed_header_against_a_valid_body_is_malformed_not_conflicting() {
    assert_eq!(
        resolve_era(
            &EnvelopeObservation::Present("not-a-date".into()),
            &RequestMetadata::Present(V2026.into())
        ),
        EraResolution::Unknown(UnknownReason::MalformedSignal)
    );
}

#[test]
fn composite_a_malformed_body_against_a_valid_header_is_malformed_not_conflicting() {
    assert_eq!(
        resolve_era(
            &EnvelopeObservation::Present(V2026.into()),
            &RequestMetadata::Present("not-a-date".into())
        ),
        EraResolution::Unknown(UnknownReason::MalformedSignal)
    );
}

#[test]
fn composite_two_malformed_signals_are_malformed() {
    assert_eq!(
        resolve_era(
            &EnvelopeObservation::Present("nope".into()),
            &RequestMetadata::Present("also-nope".into())
        ),
        EraResolution::Unknown(UnknownReason::MalformedSignal)
    );
}

/// A header that arrived and could not be read at all. Without this variant the wiring would have
/// to call it absent, which is a different fact with a different conclusion.
#[test]
fn a_malformed_header_is_representable_and_malformed() {
    assert_eq!(
        resolve_era(&EnvelopeObservation::Malformed, &RequestMetadata::Absent),
        EraResolution::Unknown(UnknownReason::MalformedSignal)
    );
}

/// Ten bytes and two dashes is not a date. An impossible month must not pass as a version this
/// build merely does not support, because that reports a reader gap where the record is wrong.
#[test]
fn an_impossible_date_is_malformed_not_unsupported() {
    for bad in ["2026-99-99", "2026-13-01", "2026-00-10", "2026-01-32"] {
        assert_eq!(
            resolve_era(
                &EnvelopeObservation::Present(bad.into()),
                &RequestMetadata::Absent
            ),
            EraResolution::Unknown(UnknownReason::MalformedSignal),
            "{bad}"
        );
    }
}

/// The twin of the metadata case, and the one an earlier fix missed. A malformed header resolves
/// to an unknown era, and reading the era alone then answers "no objection" for the one input that
/// is broken. Walks the real chain rather than stopping at `resolve_era`.
#[test]
fn composite_a_malformed_header_makes_the_request_invalid() {
    let envelope = EnvelopeObservation::Malformed;
    let metadata = RequestMetadata::Absent;
    let era = resolve_era(&envelope, &metadata);
    assert_eq!(era, EraResolution::Unknown(UnknownReason::MalformedSignal));
    assert_eq!(
        conclude_request(&era, &metadata, Some(&CapabilityObservation::CoreOnly)),
        RequestAssessment::Invalid(InvalidReason::MalformedEraSignal)
    );
}

/// The same signal on the response axis. Unreadable is invalid on both, because more evidence does
/// not make an unreadable value readable.
#[test]
fn composite_a_malformed_signal_makes_the_result_invalid() {
    let era = resolve_era(
        &EnvelopeObservation::Present("2026-02-31".into()),
        &RequestMetadata::Absent,
    );
    assert_eq!(
        conclude(&era, &ResultObservation::Complete, None),
        ResultConclusion::Invalid(InvalidReason::MalformedEraSignal)
    );
}

/// A date that is shaped right, in range per field, and does not exist. February never has 31
/// days, so this is not a version this build fails to support: it is not a version.
#[test]
fn an_impossible_day_of_month_is_malformed_not_unsupported() {
    for bad in ["2026-02-31", "2026-02-30", "2026-04-31", "2025-02-29"] {
        assert_eq!(
            resolve_era(
                &EnvelopeObservation::Present(bad.into()),
                &RequestMetadata::Absent
            ),
            EraResolution::Unknown(UnknownReason::MalformedSignal),
            "{bad}"
        );
    }
}

/// The leap-year arm, so the rule is not simply "February is 28".
#[test]
fn a_leap_day_is_a_wellformed_date() {
    assert_eq!(
        resolve_era(
            &EnvelopeObservation::Present("2024-02-29".into()),
            &RequestMetadata::Absent
        ),
        EraResolution::Unknown(UnknownReason::UnsupportedVersion("2024-02-29".into()))
    );
}

/// An unreadable `resultType` under a 2026 era.
#[test]
fn composite_a_malformed_result_type_at_2026_is_invalid() {
    let era = resolve_era(
        &EnvelopeObservation::Present(V2026.into()),
        &RequestMetadata::Present(V2026.into()),
    );
    assert_eq!(
        conclude(&era, &ResultObservation::Malformed, None),
        ResultConclusion::Invalid(InvalidReason::MalformedResultType)
    );
}

/// The arm that matters more. Under a legacy era an absent `resultType` MUST be read as complete,
/// and an unreadable one must not inherit that reading: it is not absent, it is broken. Without
/// this a malformed field silently becomes a completed action.
#[test]
fn composite_a_malformed_result_type_before_2026_is_not_complete() {
    let era = resolve_era(
        &EnvelopeObservation::Present(V2025.into()),
        &RequestMetadata::Absent,
    );
    assert_eq!(era, EraResolution::Known(V2025.into()));
    assert_eq!(
        conclude(&era, &ResultObservation::Malformed, None),
        ResultConclusion::Invalid(InvalidReason::MalformedResultType)
    );
}

/// An unreadable `resultType` is a fault whatever the era turned out to be. Reading the era first
/// downgraded it to whatever the era's own gap was, so a malformed field under an unknown era came
/// back `Incomplete` instead of `Invalid`.
#[test]
fn a_malformed_result_survives_an_unknown_era() {
    for era in [
        EraResolution::Unknown(UnknownReason::NoSignal),
        EraResolution::Unknown(UnknownReason::UnsupportedVersion("2031-01-01".into())),
    ] {
        assert_eq!(
            conclude(&era, &ResultObservation::Malformed, None),
            ResultConclusion::Invalid(InvalidReason::MalformedResultType)
        );
    }
}

/// The ASCII-digit half of the shape check, which no test reached: `"not-a-date"` dies on the
/// dash anchors and `"9999-not-a-date"` on the length, both before the digits are looked at.
/// Without it these are retained as `UnsupportedVersion`, which blames the reader and keeps
/// attacker-chosen bytes in the sidecar.
#[test]
fn a_non_numeric_date_shape_is_malformed_not_unsupported() {
    for bad in ["abcd-01-01", "20ab-01-01", "+123-01-01", "  12-01-01"] {
        assert_eq!(
            resolve_era(
                &EnvelopeObservation::Present(bad.into()),
                &RequestMetadata::Absent
            ),
            EraResolution::Unknown(UnknownReason::MalformedSignal),
            "{bad}"
        );
    }
}
