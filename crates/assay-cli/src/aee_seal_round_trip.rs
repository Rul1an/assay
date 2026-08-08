//! A run's enforcement health becomes a signed seal, and a consumer verifies it.
//!
//! #2093 records that the seal primitive was specified and tested while **nothing produced or
//! consumed one**: `git grep 'aee_seal::'` returned the `pub mod` lines in `main.rs` and nothing
//! else. The refusal path, the drop-proof models, the `checked`/`declared` basis — all library code
//! no execution path reached.
//!
//! This closes the round trip inside the repository, which #2093 asks be "exercised by a test
//! rather than by hand".
//!
//! **What #2093 said, and what is actually true now.** The issue reports that nothing produces a
//! seal, measured at `78248edd` on 2026-08-06. Re-measured before writing this: `sandbox/child.rs`
//! now calls `build_sealed_run` and `sign_seal` from `maybe_emit_aee_seal`, so
//! `assay sandbox --aee-run-context --aee-seal-key --aee-seal` already emits one and already
//! refuses when the run is not seal-eligible. That acceptance item is met on `main`; the issue's
//! grep is stale, not wrong-at-the-time.
//!
//! What was still missing is everything on this side of the wire: no consumer, and no test that the
//! round trip closes. Those are the items here.
//!
//! **Where the run is real and where it is not.** The health input is built by the same
//! constructors `sandbox/child.rs` calls, and round-tripped through JSON so the on-disk artifact
//! `--enforcement-health` writes is proven readable by the sealer rather than assumed. What is not
//! exercised is the Landlock syscall path itself, which needs Linux — that is the delegated runner
//! lane's job. So the seal is built from a real run's output shape, on a host that cannot produce
//! that output.

#![cfg(test)]

use crate::aee_seal::{
    build_sealed_run, DropAccounting, NotSealEligible, ObservationEnvironment,
    COLLECTION_PATH_LANDLOCK_TCP_CONNECT,
};
use crate::aee_seal_envelope::{
    check_substrate_scope, sign_seal, verify_seal, KeyRole, SealEnvelope, TrustedObservationKey,
};
use crate::enforcement_health_v1::{EnforcementHealthV1, Probe, ReasonCode};
use ed25519_dalek::SigningKey;

const PARITY: &str =
    include_str!("../../../scripts/experiments/fixtures/aee-landlock-seal/derivation-parity.json");
const SEALED_AT: &str = "2026-08-05T00:00:00Z";
const SUBSTRATE: &str = "assay-landlock-substrate";

/// The environment digests, from the same parity vectors the producer's own tests derive against.
fn environment() -> ObservationEnvironment {
    let p: serde_json::Value = serde_json::from_str(PARITY).expect("parity vectors parse");
    let e = &p["environment"];
    let g = |k: &str| e[k].as_str().expect("digest").to_string();
    ObservationEnvironment {
        subject_digest: g("subject"),
        substrate_digest: g("substrate"),
        corpus_digest: g("corpus"),
        catch_policy_digest: g("catchPolicy"),
        observation_vocabulary_digest: g("observationVocabulary"),
        run_entropy_digest: g("runEntropy"),
        network_posture: e["networkPosture"].clone(),
    }
}

/// A run that armed Landlock at ABI 4 and proved it: a connect refused with EACCES at run end.
///
/// Built with the constructor `sandbox/child.rs:169` calls, then sent through JSON and back. The
/// detour is the point: `--enforcement-health` writes this artifact to disk and the sealer reads it
/// from there, so a field that does not survive serialisation would break the real chain while a
/// constructor-only test stayed green.
fn armed_run() -> EnforcementHealthV1 {
    on_disk(EnforcementHealthV1::landlock_active(
        4,
        vec![443],
        Some(Probe {
            kind: "real_block".into(),
            transport: "ipv4".into(),
            blocked_action: "tcp_connect".into(),
            blocked_port: 4444,
            blocked_errno: "EACCES".into(),
            listener_reached: false,
        }),
        Some("restrictions_held".to_string()),
    ))
}

/// The same run with no run-end probe: armed, and nothing showing that it held.
fn armed_without_probe() -> EnforcementHealthV1 {
    on_disk(EnforcementHealthV1::landlock_active(
        4,
        vec![443],
        None,
        Some("restrictions_held".to_string()),
    ))
}

/// Enforcement requested and not installed.
fn never_armed() -> EnforcementHealthV1 {
    on_disk(EnforcementHealthV1::landlock_failed(
        4,
        ReasonCode::RestrictSelfFailed,
        "landlock restrict_self failed in the enforcing child",
        true,
        true,
    ))
}

/// Through the on-disk representation and back, asserting nothing is lost on the way.
fn on_disk(health: EnforcementHealthV1) -> EnforcementHealthV1 {
    let json = serde_json::to_string(&health).expect("health serialises as sandbox writes it");
    let back: EnforcementHealthV1 =
        serde_json::from_str(&json).expect("and the sealer reads it back");
    assert_eq!(
        back, health,
        "the artifact must survive the trip to the sealer"
    );
    back
}

fn signing_key() -> SigningKey {
    SigningKey::from_bytes(&[7u8; 32])
}

/// One key, scoped to whichever collection paths the caller names.
fn trusted(k: &SigningKey, paths: &[&str]) -> TrustedObservationKey {
    TrustedObservationKey {
        keyid: "observer-1".into(),
        role: KeyRole::SubstrateObservation,
        verifying_key: k.verifying_key(),
        collection_paths: paths.iter().map(|p| (*p).to_string()).collect(),
        substrate: SUBSTRATE.into(),
    }
}

fn seal_and_sign(health: &EnforcementHealthV1, path: &str, k: &SigningKey) -> SealEnvelope {
    let sealed = build_sealed_run(
        health,
        &environment(),
        &[],
        SEALED_AT,
        &DropAccounting::SynchronousProbe,
        path,
    )
    .expect("an armed run with a run-end block is seal-eligible");
    sign_seal(&sealed.seal, k, "observer-1", KeyRole::SubstrateObservation)
        .expect("the production envelope signs it")
}

#[test]
fn a_run_emits_a_signed_seal_and_a_consumer_verifies_it() {
    let health = armed_run();
    let k = signing_key();

    let envelope = seal_and_sign(&health, COLLECTION_PATH_LANDLOCK_TCP_CONNECT, &k);
    let trusted = trusted(&k, &[COLLECTION_PATH_LANDLOCK_TCP_CONNECT]);

    check_substrate_scope(&trusted, SUBSTRATE).expect("the statement's substrate is in scope");
    let payload = verify_seal(&envelope, &trusted).expect("a consumer verifies the seal");

    assert!(
        payload.aee_still_armed,
        "the run was still armed at seal time"
    );
    assert_eq!(payload.aee_drop_count, 0);
    assert_eq!(
        payload.assay_collection_path,
        COLLECTION_PATH_LANDLOCK_TCP_CONNECT
    );
    assert_eq!(
        payload.assay_drop_proof_basis, "asserted",
        "a synchronous probe verified nothing about a queue, and says so"
    );
}

/// A tampered envelope does not verify. Without this the test above would hold for a verifier that
/// checked nothing at all.
#[test]
fn an_edited_seal_does_not_verify() {
    let health = armed_run();
    let k = signing_key();
    let mut envelope = seal_and_sign(&health, COLLECTION_PATH_LANDLOCK_TCP_CONNECT, &k);

    let decoded = envelope.payload.clone();
    envelope.payload = decoded.replace('0', "1");
    assert_ne!(envelope.payload, decoded, "the edit must have applied");

    verify_seal(
        &envelope,
        &trusted(&k, &[COLLECTION_PATH_LANDLOCK_TCP_CONNECT]),
    )
    .expect_err("an edited payload must not verify");
}

/// Ineligibility refuses the run rather than emitting a guessed zero.
///
/// #2093 names this as its own acceptance item because the refusal is code that had never executed:
/// twelve `NotSealEligible` variants, none reachable from any run. Two are exercised here, chosen
/// because they fail for opposite reasons — one where enforcement was never armed, and one where it
/// was armed but produced no evidence that it held.
#[test]
fn a_run_that_cannot_prove_enforcement_is_refused_rather_than_sealed() {
    let never_armed = never_armed();
    match build_sealed_run(
        &never_armed,
        &environment(),
        &[],
        SEALED_AT,
        &DropAccounting::SynchronousProbe,
        COLLECTION_PATH_LANDLOCK_TCP_CONNECT,
    ) {
        Err(NotSealEligible::NotArmed { .. }) => {}
        other => panic!("a run that never armed must refuse, got {other:?}"),
    }

    let no_probe = armed_without_probe();
    match build_sealed_run(
        &no_probe,
        &environment(),
        &[],
        SEALED_AT,
        &DropAccounting::SynchronousProbe,
        COLLECTION_PATH_LANDLOCK_TCP_CONNECT,
    ) {
        Err(NotSealEligible::NoRunEndProbe) => {}
        other => panic!("an armed run with no run-end probe must refuse, got {other:?}"),
    }
}

/// Two collection paths, one substrate, one key.
///
/// #2093 states the constraint as coming from the predicate author: two keys makes two substrates
/// and the run binding stops resolving. The consumer side has always been able to express this —
/// `TrustedObservationKey::collection_paths` is a `Vec` — while the producer could only ever name
/// one path, which `aee_seal.rs` records as "a capability gap, not a design position".
///
/// What this proves: the substrate and key model admit a second path today. What it does not
/// prove: that a second *collector* exists. The JSON-RPC proxy is not built, so the second path
/// here is named rather than collected from, and the honest reading is that the key scope is ready
/// and the vantage is not.
#[test]
fn a_second_collection_path_verifies_under_the_same_key_and_substrate() {
    const SECOND_PATH: &str = "jsonrpc-proxy";
    let health = armed_run();
    let k = signing_key();

    let first = seal_and_sign(&health, COLLECTION_PATH_LANDLOCK_TCP_CONNECT, &k);
    let second = seal_and_sign(&health, SECOND_PATH, &k);

    let one_key = trusted(&k, &[COLLECTION_PATH_LANDLOCK_TCP_CONNECT, SECOND_PATH]);
    let a = verify_seal(&first, &one_key).expect("first path verifies");
    let b = verify_seal(&second, &one_key).expect("second path verifies under the same key");

    assert_eq!(
        a.assay_collection_path,
        COLLECTION_PATH_LANDLOCK_TCP_CONNECT
    );
    assert_eq!(b.assay_collection_path, SECOND_PATH);
    assert_eq!(
        a.aee_run_binding, b.aee_run_binding,
        "one substrate, one run: the binding must resolve to the same value on both paths, which \
         is what a second key would break"
    );

    // And the scope is a real constraint, not decoration: a key that names only the first path
    // must refuse the second.
    let narrow = trusted(&k, &[COLLECTION_PATH_LANDLOCK_TCP_CONNECT]);
    verify_seal(&second, &narrow).expect_err("a path outside the key's scope must be refused");
}
