//! ADR-043 §3: fuzz the evidence-chain verifier, which is the artifact the profile is about.
//!
//! This target previously pointed at `assay_core::replay::verify_bundle`. That is a different
//! bundle format with a different reader, so the golden path had no fuzz coverage at all while
//! appearing to have some.
//!
//! The fuzzer's job here is narrow and deliberately so: find inputs that crash, hang, or push the
//! verifier past its own resource ceilings. It asserts nothing about *which* rejection is correct,
//! because an arbitrary byte string has no expected verdict. The semantic oracle lives next to the
//! verifier in `crates/assay-evidence/tests/verifier_fail_closed_properties.rs`, where inputs are
//! named and their `ErrorCode` is pinned.

#![no_main]

use assay_evidence::{verify_bundle_with_limits, VerifyLimits};
use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

/// Small explicit limits, matching the deterministic property tests.
///
/// The defaults allow a 100 MB input and 1 GB of expansion, which would let a single case burn the
/// whole run budget and would mean an OOM told us about the machine rather than the verifier.
///
/// `-rss_limit_mb` in the lane is an outer process guard, not evidence about these numbers: it
/// observes whole-process RSS and cannot show that this reader honoured a 1 MiB input or 4 MiB
/// decode ceiling. The evidence for the ceilings themselves is the exact-limit and classification
/// assertions in `verifier_fail_closed_properties.rs`.
fn limits() -> VerifyLimits {
    VerifyLimits {
        max_bundle_bytes: 1 << 20,
        max_decode_bytes: 4 << 20,
        max_manifest_bytes: 64 << 10,
        max_events_bytes: 256 << 10,
        max_events: 64,
        max_line_bytes: 8 << 10,
        max_path_len: 128,
        max_json_depth: 16,
    }
}

fuzz_target!(|data: &[u8]| {
    // Both arms are acceptable outcomes. The bug classes this target hunts are a panic, a hang, or
    // memory growth past the ceilings above -- never a particular verdict.
    let _ = verify_bundle_with_limits(Cursor::new(data), limits());
});
