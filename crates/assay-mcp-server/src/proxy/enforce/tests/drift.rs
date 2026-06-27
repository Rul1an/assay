use super::fixtures::*;
use super::*;

// --- c3 drift gate (runs only after classification + allowance + credential-scope pass) ---------

/// `decide` for the acme call with VALID, a given baseline, and a given observed digest state.
fn decide_drift(baseline: &DeclaredManifest, observed: &ObservedToolDigest) -> Decision {
    let p = policy_from(VALID).unwrap();
    decide(
        &p,
        baseline,
        observed,
        "github.add_deploy_key",
        &acme_call(),
    )
}

#[test]
fn baseline_missing_when_tool_absent_from_baseline() {
    // A baseline that has SOME tool but not the invoked one -> this tool has no approved baseline.
    let baseline = baseline_with("other.tool", APPROVED);
    let d = decide_drift(
        &baseline,
        &ObservedToolDigest::Present(APPROVED.to_string()),
    );
    assert!(!d.allow);
    assert_eq!(d.reason, "manifest_baseline_missing");
}

#[test]
fn current_observation_incomplete_when_no_complete_manifest() {
    let baseline = baseline_with(TOOL, APPROVED);
    let d = decide_drift(&baseline, &ObservedToolDigest::NoCompleteManifest);
    assert!(!d.allow);
    assert_eq!(d.reason, "manifest_current_observation_incomplete");
}

#[test]
fn current_observation_incomplete_when_tool_absent_from_observed() {
    // A complete manifest that does not contain the invoked tool: no current digest to compare,
    // so drift cannot be ruled out -> fail closed (not an allow).
    let baseline = baseline_with(TOOL, APPROVED);
    let d = decide_drift(&baseline, &ObservedToolDigest::CompleteButToolAbsent);
    assert!(!d.allow);
    assert_eq!(d.reason, "manifest_current_observation_incomplete");
}

#[test]
fn ambiguous_observation_denies_observation_ambiguous() {
    // A duplicate-name (ambiguous) observed manifest is inconclusive -> deny, never pick a digest.
    let baseline = baseline_with(TOOL, APPROVED);
    let d = decide_drift(&baseline, &ObservedToolDigest::Ambiguous);
    assert!(!d.allow);
    assert_eq!(d.reason, "manifest_observation_ambiguous");
}

#[test]
fn drifted_when_observed_digest_differs_from_baseline() {
    let baseline = baseline_with(TOOL, APPROVED);
    let d = decide_drift(
        &baseline,
        &ObservedToolDigest::Present("sha256:something-else".to_string()),
    );
    assert!(!d.allow);
    assert_eq!(d.reason, "manifest_drifted_since_approval");
}

#[test]
fn allows_only_when_baseline_and_observed_digests_match_exactly() {
    let baseline = baseline_with(TOOL, APPROVED);
    let d = decide_drift(
        &baseline,
        &ObservedToolDigest::Present(APPROVED.to_string()),
    );
    assert!(
        d.allow,
        "exact digest match clears the drift gate -> forward"
    );
    assert_eq!(d.reason, "allow");
}
