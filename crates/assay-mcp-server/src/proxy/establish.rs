// Pre-call manifest-establish decision logic (P61e, Increment 1).
//
// This module is the PURE decision layer for the pre-call manifest-establish flow described in the
// review-spec: before `enforce::decide()`'s drift gate runs on a privileged `tools/call`, decide
// whether the proxy should try to establish a current complete manifest (a bounded re-list) or deny
// immediately, and classify which establish path the call took for the sibling observability carrier
// `assay.manifest_establish.v0`.
//
// Increment 1 scope (deliberate): no proxy-originated `tools/list` plumbing, no change to
// `enforce::decide()`, no change to the live relay, and no change to the pinned
// `assay.enforcement_decision.v0` contract. The live re-list and the emission of this carrier on the
// relay path land in Increment 2. The items here are therefore not yet wired into the binary's run
// path, hence `allow(dead_code)`; they are fully exercised by the unit tests below and consumed by the
// relay in Increment 2.
#![allow(dead_code)]

use serde_json::{json, Value};

use super::enforce::ObservedToolDigest;

/// Sibling observability carrier schema. Deliberately SEPARATE from `assay.enforcement_decision.v0`
/// so that pinned producer/consumer contract stays byte-identical (no re-vendor, no consumer churn).
pub const MANIFEST_ESTABLISH_SCHEMA: &str = "assay.manifest_establish.v0";

/// What the establish step should do for a given observed manifest state, BEFORE the drift gate.
///
/// The mapping is fail-closed and never lets establish overreach: it only targets the observed-side
/// availability gap. It never tries to supply a declared baseline, resolve an ambiguous observation, or
/// clear real drift — those remain `enforce::decide()`'s job and stay denials.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstablishAction {
    /// A current complete observation of this tool already exists: no establish needed.
    NotNeeded,
    /// No current complete observation (or one invalidated by a later `tools/list_changed`): a bounded
    /// re-list may supply one.
    ReList,
    /// A complete manifest was observed but the invoked tool is absent: re-list ONCE (the tool may have
    /// been added since the last list), deny if still absent.
    ReListOnce,
    /// An inconclusive/structural state establish cannot resolve (duplicate-name ambiguity): deny
    /// without an establish attempt, never allow.
    DenyWithoutEstablish,
}

/// Decide the establish action for an observed manifest state. Pure; no I/O.
pub fn establish_action(observed: &ObservedToolDigest) -> EstablishAction {
    match observed {
        ObservedToolDigest::Present(_) => EstablishAction::NotNeeded,
        ObservedToolDigest::NoCompleteManifest => EstablishAction::ReList,
        ObservedToolDigest::CompleteButToolAbsent => EstablishAction::ReListOnce,
        ObservedToolDigest::Ambiguous => EstablishAction::DenyWithoutEstablish,
    }
}

/// The result of an establish attempt. Increment 1's run path only ever produces `NotPerformed`
/// (no live re-list yet); the other two model Increment 2's outcomes so the path resolution and its
/// never-allow invariant are testable now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstablishOutcome {
    /// No establish was attempted (Increment 1 default, or `EstablishAction::DenyWithoutEstablish`).
    NotPerformed,
    /// A fresh complete, terminal, non-ambiguous observation was obtained, WHETHER OR NOT it contains
    /// the invoked tool. A complete re-list that simply lacks the tool is NOT a failure here: it is
    /// `EstablishedComplete`, and `decide()` then denies (yielding `EstablishedThenDenied`) (Increment 2).
    EstablishedComplete,
    /// The establish attempt failed to produce a usable complete observation: timeout, a
    /// partial/incomplete list, a transport error, or an unusable (e.g. ambiguous) observation. Tool
    /// absence from an otherwise complete list is NOT a failure (see `EstablishedComplete`) (Increment 2).
    EstablishFailed,
}

/// The path a privileged call took through the establish step, for the sibling carrier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstablishPath {
    /// A current complete observation already existed; the establish step was a no-op.
    NoEstablishNeeded,
    /// Establish produced a complete observation and the re-evaluated decision allowed.
    EstablishedThenAllowed,
    /// Establish produced a complete observation but the re-evaluated decision still denied
    /// (e.g. real drift, or tool still absent).
    EstablishedThenDenied,
    /// Denied without (or despite a failed) establish attempt.
    ImmediateDeny,
}

impl EstablishPath {
    pub fn as_str(self) -> &'static str {
        match self {
            EstablishPath::NoEstablishNeeded => "no_establish_needed",
            EstablishPath::EstablishedThenAllowed => "established_then_allowed",
            EstablishPath::EstablishedThenDenied => "established_then_denied",
            EstablishPath::ImmediateDeny => "immediate_deny",
        }
    }
}

/// Resolve the establish path from the action, the attempt outcome, and the re-evaluated decision.
///
/// Load-bearing invariant: the only ESTABLISH-DERIVED allow path is `EstablishedThenAllowed`, and it
/// requires BOTH an `EstablishedComplete` outcome AND a `decide_allowed` re-evaluation. A not-performed
/// or failed establish therefore never produces an establish-derived allow, and `DenyWithoutEstablish`
/// (ambiguous) never allows regardless of outcome.
///
/// `NoEstablishNeeded` is ORTHOGONAL to the verdict, not a deny: when a current complete manifest
/// already exists, the call takes `NoEstablishNeeded` and the separate `assay.enforcement_decision.v0`
/// record carries the allow or deny. So `NoEstablishNeeded` + an allowed enforcement decision is a valid
/// combination. This carrier describes the establish JOURNEY only; it is never a verdict proxy.
pub fn establish_path(
    action: EstablishAction,
    outcome: EstablishOutcome,
    decide_allowed: bool,
) -> EstablishPath {
    match action {
        EstablishAction::NotNeeded => EstablishPath::NoEstablishNeeded,
        EstablishAction::DenyWithoutEstablish => EstablishPath::ImmediateDeny,
        EstablishAction::ReList | EstablishAction::ReListOnce => match outcome {
            EstablishOutcome::EstablishedComplete if decide_allowed => {
                EstablishPath::EstablishedThenAllowed
            }
            EstablishOutcome::EstablishedComplete => EstablishPath::EstablishedThenDenied,
            EstablishOutcome::NotPerformed | EstablishOutcome::EstablishFailed => {
                EstablishPath::ImmediateDeny
            }
        },
    }
}

/// The diagnostic outcome of the establish run for the carrier. `not_performed` when no establish ran
/// (the carrier's `establish_attempted` is derived from this). The other values are the snake_case
/// `EstablishRunOutcome` variants. It is journey/operability only, NEVER a verdict — the allow/deny
/// lives in `assay.enforcement_decision.v0`.
pub const RUN_OUTCOME_NOT_PERFORMED: &str = "not_performed";

/// Build the sibling `assay.manifest_establish.v0` carrier record. Kept intentionally small and
/// separate from the enforcement-decision record. `establish_attempted` is DERIVED from `run_outcome`
/// (true iff an establish actually ran), so the two fields can never disagree — the invariant
/// `establish_attempted == (run_outcome != "not_performed")` holds by construction.
pub fn build_manifest_establish_record(
    path: EstablishPath,
    action_class: Option<&str>,
    run_outcome: &str,
) -> Value {
    let establish_attempted = run_outcome != RUN_OUTCOME_NOT_PERFORMED;
    json!({
        "schema": MANIFEST_ESTABLISH_SCHEMA,
        "establish_path": path.as_str(),
        "establish_attempted": establish_attempted,
        "action_class": action_class,
        "run_outcome": run_outcome,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- the agreed establish-action table ---

    #[test]
    fn no_complete_manifest_triggers_relist() {
        assert_eq!(
            establish_action(&ObservedToolDigest::NoCompleteManifest),
            EstablishAction::ReList
        );
    }

    #[test]
    fn complete_but_tool_absent_triggers_relist_once() {
        assert_eq!(
            establish_action(&ObservedToolDigest::CompleteButToolAbsent),
            EstablishAction::ReListOnce
        );
    }

    #[test]
    fn ambiguous_is_immediate_deny_no_establish() {
        assert_eq!(
            establish_action(&ObservedToolDigest::Ambiguous),
            EstablishAction::DenyWithoutEstablish
        );
    }

    #[test]
    fn present_needs_no_establish() {
        assert_eq!(
            establish_action(&ObservedToolDigest::Present("sha256:t".to_string())),
            EstablishAction::NotNeeded
        );
    }

    // --- path resolution ---

    #[test]
    fn not_needed_path_is_no_establish_needed() {
        assert_eq!(
            establish_path(
                EstablishAction::NotNeeded,
                EstablishOutcome::NotPerformed,
                true
            ),
            EstablishPath::NoEstablishNeeded
        );
    }

    #[test]
    fn no_establish_needed_coexists_with_allow() {
        // A current complete manifest already exists, so no establish runs and the call takes
        // NoEstablishNeeded REGARDLESS of the enforcement verdict. NoEstablishNeeded is verdict-
        // orthogonal: it is never ImmediateDeny, and `NoEstablishNeeded` + an allowed enforcement
        // decision (carried separately in assay.enforcement_decision.v0) is a valid combination.
        for outcome in [
            EstablishOutcome::NotPerformed,
            EstablishOutcome::EstablishFailed,
            EstablishOutcome::EstablishedComplete,
        ] {
            for decide_allowed in [true, false] {
                let path = establish_path(EstablishAction::NotNeeded, outcome, decide_allowed);
                assert_eq!(path, EstablishPath::NoEstablishNeeded);
                assert_ne!(path, EstablishPath::ImmediateDeny);
            }
        }
    }

    #[test]
    fn ambiguous_path_is_immediate_deny_regardless_of_outcome() {
        for outcome in [
            EstablishOutcome::NotPerformed,
            EstablishOutcome::EstablishFailed,
            EstablishOutcome::EstablishedComplete,
        ] {
            for allowed in [true, false] {
                assert_eq!(
                    establish_path(EstablishAction::DenyWithoutEstablish, outcome, allowed),
                    EstablishPath::ImmediateDeny
                );
            }
        }
    }

    #[test]
    fn established_complete_and_allowed_is_established_then_allowed() {
        for action in [EstablishAction::ReList, EstablishAction::ReListOnce] {
            assert_eq!(
                establish_path(action, EstablishOutcome::EstablishedComplete, true),
                EstablishPath::EstablishedThenAllowed
            );
        }
    }

    #[test]
    fn established_complete_but_denied_is_established_then_denied() {
        for action in [EstablishAction::ReList, EstablishAction::ReListOnce] {
            assert_eq!(
                establish_path(action, EstablishOutcome::EstablishedComplete, false),
                EstablishPath::EstablishedThenDenied
            );
        }
    }

    #[test]
    fn relist_without_or_failed_establish_is_immediate_deny() {
        for action in [EstablishAction::ReList, EstablishAction::ReListOnce] {
            for outcome in [
                EstablishOutcome::NotPerformed,
                EstablishOutcome::EstablishFailed,
            ] {
                for allowed in [true, false] {
                    assert_eq!(
                        establish_path(action, outcome, allowed),
                        EstablishPath::ImmediateDeny
                    );
                }
            }
        }
    }

    // --- acceptance invariants ---

    #[test]
    fn failed_or_unavailable_establish_never_allows() {
        // Across the whole cross-product, the only allow path requires EstablishedComplete AND
        // decide_allowed; a not-performed or failed establish must never produce an allow path.
        for action in [
            EstablishAction::NotNeeded,
            EstablishAction::ReList,
            EstablishAction::ReListOnce,
            EstablishAction::DenyWithoutEstablish,
        ] {
            for outcome in [
                EstablishOutcome::NotPerformed,
                EstablishOutcome::EstablishFailed,
                EstablishOutcome::EstablishedComplete,
            ] {
                for allowed in [true, false] {
                    let path = establish_path(action, outcome, allowed);
                    if path == EstablishPath::EstablishedThenAllowed {
                        assert_eq!(outcome, EstablishOutcome::EstablishedComplete);
                        assert!(allowed);
                        assert!(matches!(
                            action,
                            EstablishAction::ReList | EstablishAction::ReListOnce
                        ));
                    }
                }
            }
        }
    }

    #[test]
    fn establish_never_resolves_ambiguous_baseline_or_drift() {
        // Ambiguity is the only ObservedToolDigest state establish can see that is inconclusive;
        // it must always map to a non-establishing deny. Baseline-missing and real drift are decided
        // by enforce::decide() over a Present digest, so establish (which only yields Present) can
        // never be the thing that clears them: it has no DenyWithoutEstablish escape hatch back to allow.
        assert_eq!(
            establish_action(&ObservedToolDigest::Ambiguous),
            EstablishAction::DenyWithoutEstablish
        );
        // Even a "successful" establish on an ambiguous action cannot allow.
        assert_eq!(
            establish_path(
                EstablishAction::DenyWithoutEstablish,
                EstablishOutcome::EstablishedComplete,
                true
            ),
            EstablishPath::ImmediateDeny
        );
    }

    // --- sibling carrier ---

    #[test]
    fn carrier_record_uses_sibling_schema_and_does_not_touch_enforcement_decision() {
        let rec = build_manifest_establish_record(
            EstablishPath::EstablishedThenAllowed,
            Some("github_deploy_key"),
            "complete",
        );
        assert_eq!(rec["schema"], json!("assay.manifest_establish.v0"));
        assert_ne!(rec["schema"], json!("assay.enforcement_decision.v0"));
        assert_eq!(rec["establish_path"], json!("established_then_allowed"));
        assert_eq!(rec["run_outcome"], json!("complete"));
        // establish_attempted is DERIVED from run_outcome -> true here (an establish ran).
        assert_eq!(rec["establish_attempted"], json!(true));
        assert_eq!(rec["action_class"], json!("github_deploy_key"));
    }

    #[test]
    fn carrier_record_derives_not_performed_as_not_attempted() {
        // run_outcome "not_performed" must derive establish_attempted = false (the invariant).
        let rec = build_manifest_establish_record(
            EstablishPath::NoEstablishNeeded,
            Some("github_deploy_key"),
            "not_performed",
        );
        assert_eq!(rec["run_outcome"], json!("not_performed"));
        assert_eq!(rec["establish_attempted"], json!(false));
        // immediate_deny with no establish (ambiguous) is also not_performed -> not attempted.
        let rec2 =
            build_manifest_establish_record(EstablishPath::ImmediateDeny, None, "not_performed");
        assert_eq!(rec2["establish_attempted"], json!(false));
        assert_eq!(rec2["action_class"], json!(null));
    }

    #[test]
    fn carrier_path_strings_are_stable() {
        assert_eq!(
            EstablishPath::NoEstablishNeeded.as_str(),
            "no_establish_needed"
        );
        assert_eq!(
            EstablishPath::EstablishedThenAllowed.as_str(),
            "established_then_allowed"
        );
        assert_eq!(
            EstablishPath::EstablishedThenDenied.as_str(),
            "established_then_denied"
        );
        assert_eq!(EstablishPath::ImmediateDeny.as_str(), "immediate_deny");
    }
}
