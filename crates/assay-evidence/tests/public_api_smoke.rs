//! Compile-test for bundle façade: ensures public API symbols are re-exported.
//!
//! Prevents regressions when refactoring module structure.

#[test]
fn public_api_smoke_bundle_facade() {
    use assay_evidence::bundle::{
        verify_bundle, verify_bundle_with_limits, BundleProvenance, BundleWriter, ErrorClass,
        ErrorCode, Manifest, ProvenanceInput, VerifyError, VerifyLimits, VerifyLimitsOverrides,
        XAssayExtension,
    };

    let _ = VerifyLimits::default().apply(VerifyLimitsOverrides::default());

    // Type-check: functions exist and have expected signatures
    let _ = verify_bundle as fn(std::io::Cursor<Vec<u8>>) -> _;
    let _ = verify_bundle_with_limits as fn(std::io::Cursor<Vec<u8>>, VerifyLimits) -> _;

    // Symbols exist; no runtime needed
    let _ = std::mem::size_of::<Manifest>();
    let _ = std::mem::size_of::<BundleWriter<Vec<u8>>>();
    let _ = std::mem::size_of::<XAssayExtension>();
    let _ = std::mem::size_of::<BundleProvenance>();
    let _ = std::mem::size_of::<ProvenanceInput>();
    let _ = std::mem::size_of::<ErrorClass>();
    let _ = std::mem::size_of::<ErrorCode>();
    let _ = std::mem::size_of::<VerifyError>();
}
