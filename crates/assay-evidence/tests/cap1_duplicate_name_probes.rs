//! The five CAP-1 duplicate-name probes of certisyn-drafts#11, run through the normative
//! verifier.
//!
//! Each probe repeats one member name in one object. A last-wins reader keeps the final value
//! and a first-wins reader the first, so S13 (top-level `profile`) and S14
//! (`subject.digest.algorithm`) read differently depending on the parser. CAP-1 requires the
//! repeated name to be refused during parsing; refusal by a later rule does not show that.
//!
//! Fixtures under `tests/fixtures/cap1/probes/` are vendored byte-for-byte and pinned by
//! `SHA256SUMS`.

use std::fs;
use std::path::{Path, PathBuf};

use assay_evidence::{
    verify_cap1_document, verify_cap1_rules, Cap1AdmissionLimits, Cap1Document, Cap1Refusal,
    Cap1SyntaxFault,
};
use sha2::{Digest, Sha256};

const PROBES: [&str; 5] = [
    "S11_same_value",
    "S12_escaped_spelling",
    "S13_top_level_profile",
    "S14_deep_algorithm",
    "S15_threefold_complete",
];

fn probe_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cap1/probes")
}

fn probe(name: &str) -> Vec<u8> {
    fs::read(probe_dir().join(format!("{name}.json"))).expect("probe readable")
}

#[test]
fn vendored_probes_match_their_recorded_digests() {
    let sums = fs::read_to_string(probe_dir().join("SHA256SUMS")).expect("SHA256SUMS");
    let mut seen = 0;
    for line in sums.lines() {
        let (digest, name) = line.split_once("  ").expect("two-space shasum format");
        let bytes = fs::read(probe_dir().join(name)).expect("listed probe exists");
        assert_eq!(hex::encode(Sha256::digest(&bytes)), digest, "{name}");
        seen += 1;
    }
    assert_eq!(seen, PROBES.len());
}

#[test]
fn every_probe_is_refused_for_its_repeated_name() {
    for name in PROBES {
        let err =
            verify_cap1_document(&probe(name), &Cap1AdmissionLimits::default()).expect_err(name);
        assert_eq!(
            err,
            Cap1Refusal::Syntax(Cap1SyntaxFault::DuplicateKey),
            "{name} must be refused at strict syntax, not by a later stage"
        );
    }
}

/// Positive control: the stages after the strict scan, fed the same bytes, accept S11, S13
/// and S14 with the last value kept. Without the scan those three would conform, so the
/// refusal above comes from duplicate detection and not from the schema or a rule.
#[test]
fn without_the_strict_scan_three_probes_would_conform() {
    for name in [
        "S11_same_value",
        "S13_top_level_profile",
        "S14_deep_algorithm",
    ] {
        let instance: serde_json::Value =
            serde_json::from_slice(&probe(name)).expect("last-wins parse accepts it");
        let doc: Cap1Document = serde_json::from_value(instance).expect("typed shape");
        verify_cap1_rules(&doc).unwrap_or_else(|e| panic!("{name} rules: {e}"));
    }
    let s14: serde_json::Value =
        serde_json::from_slice(&probe("S14_deep_algorithm")).expect("parses");
    assert_eq!(s14["subject"]["digest"]["algorithm"], "MD5");
}
