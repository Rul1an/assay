//! Golden contract for `assay.render_safety_conformance.v0` (MCP01a).
//!
//! Pins the render-sink conformance report produced by the real render-safety pipeline over the
//! shared corpus. Regenerate intentionally with `ASSAY_UPDATE_GOLDEN=1`.

use assay_core::render_safety::conformance::{is_clean, run_conformance, RenderSafetyConformance};

const GOLDEN: &str = "tests/fixtures/render_safety_conformance.v0.golden.json";

#[test]
fn conformance_matches_golden_fixture() {
    let report = run_conformance();
    let rendered = format!("{}\n", serde_json::to_string_pretty(&report).unwrap());
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN);

    if std::env::var("ASSAY_UPDATE_GOLDEN").is_ok() {
        std::fs::write(&path, &rendered).unwrap();
    }

    let golden = std::fs::read_to_string(&path).expect("golden fixture exists");
    let golden_report: RenderSafetyConformance = serde_json::from_str(&golden).unwrap();
    assert_eq!(
        report, golden_report,
        "render-safety conformance drifted from golden"
    );
    assert!(
        is_clean(&golden_report),
        "committed golden conformance is not clean"
    );
}
