//! ADR-043 §3: fuzz the incident-package verifier, which receivers run on hostile
//! bytes. Coverage of the evidence-chain bundle reader does not count.
//!
//! The fuzzer's job here is narrow: find inputs that crash, hang, or push the
//! verifier past its own resource ceilings. It asserts nothing about *which*
//! rejection is correct, because an arbitrary byte string has no expected
//! verdict. The semantic oracle lives in
//! `crates/assay-evidence/tests/incident_fail_closed_properties.rs`.
//!
//! Contexts are a fixed set, not a selector byte, so seed files stay
//! byte-identical to the fixture tars:
//! - C0 = `ContextInput::default()`, the bound context for the synthetic valid
//!   and named-refusal fixtures.
//! - C1 = the #3345 combined-session context, parsed from the pinned fixture
//!   JSON. That package is bound to C1; C0 cannot verify it.
//! - C2 = the attestation context, parsed from the pinned fixture JSON. Its
//!   `keys` selects the test key that signed `attestation_verified.tar`. C0 and
//!   C1 select no key, so under them every attestation refuses `trust_input`
//!   before PEM, DSSE, signature or report checks run; C2 is what lets fuzzed
//!   inputs reach those checks.
//!
//! Limits stay the values those contexts already carry (HARD). `ContextLimits`
//! are fields of `ContextInput`, and the assessment binds
//! `verification_context_sha256` over the whole context (SPEC §7.4; fixture
//! README: a recipient cannot change limits without changing the bound
//! assessment). Overlaying the smaller N1 `PINNED_COVERED_LIMITS` usages, or
//! any other numbers, without rebinding is `stale_assessment`, so every valid
//! seed would fail the coverage table. That is the opposite of
//! `bundle_reader`'s `VerifyLimits`, which are not part of the artifact.
//! libFuzzer's `-max_len=65536` is the outer-byte cap on generated inputs.

#![no_main]

use assay_evidence::{
    verify_incident_package, ContextInput, IncidentOutcome, IncidentReason, IncidentVerifyReport,
    INCIDENT_VERIFY_SCHEMA_V1, NON_CLAIMS_DEFAULT,
};
use libfuzzer_sys::fuzz_target;
use std::sync::OnceLock;

const SESSION_3345_CONTEXT_JSON: &[u8] = include_bytes!(
    "../../crates/assay-evidence/tests/fixtures/incident_package/combined_session_3345.context.json"
);
const ATTESTATION_CONTEXT_JSON: &[u8] = include_bytes!(
    "../../crates/assay-evidence/tests/fixtures/incident_package/attestation_verified.context.json"
);

fn contexts() -> &'static [ContextInput; 3] {
    static CONTEXTS: OnceLock<[ContextInput; 3]> = OnceLock::new();
    CONTEXTS.get_or_init(|| {
        let c0 = ContextInput::default();
        let c1: ContextInput = serde_json::from_slice(SESSION_3345_CONTEXT_JSON)
            .expect("combined_session_3345.context.json is ContextInput");
        let c2: ContextInput = serde_json::from_slice(ATTESTATION_CONTEXT_JSON)
            .expect("attestation_verified.context.json is ContextInput");
        [c0, c1, c2]
    })
}

/// SPEC-Incident-Package-v1 §7.4 report-shape invariants. These hold for any
/// input; they are not a verdict about the bytes.
fn assert_report_invariants(report: &IncidentVerifyReport) {
    // §7.4: `schema`=assay.incident.verify.v1
    assert_eq!(report.schema, INCIDENT_VERIFY_SCHEMA_V1);
    // §7.4: "non_claims is always the §7 constant."
    assert!(report
        .non_claims
        .iter()
        .map(String::as_str)
        .eq(NON_CLAIMS_DEFAULT.iter().copied()));
    // §7.4 table: package_verified / null; package_refused / first §10 reason;
    // verification_unavailable / io_unavailable. The report type's `reason`
    // doc restates the same pairing ("if refused or unavailable").
    match report.outcome {
        IncidentOutcome::PackageVerified => assert_eq!(report.reason, None),
        IncidentOutcome::PackageRefused => assert!(report.reason.is_some()),
        IncidentOutcome::VerificationUnavailable => {
            assert_eq!(report.reason, Some(IncidentReason::IoUnavailable));
        }
    }
}

fn verify_canonical(data: &[u8], context: &ContextInput) -> Vec<u8> {
    let report = verify_incident_package(data, context);
    assert_report_invariants(&report);
    report
        .to_canonical_bytes()
        .expect("incident verify report has canonical JCS bytes")
}

fuzz_target!(|data: &[u8]| {
    for context in contexts() {
        let first = verify_canonical(data, context);
        let second = verify_canonical(data, context);
        assert_eq!(
            first, second,
            "incident-package verify is not deterministic"
        );
    }
});
