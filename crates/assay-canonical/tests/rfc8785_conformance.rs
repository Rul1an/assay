//! RFC 8785 (JCS) conformance for this crate's canonicalizer.
//!
//! Every content id, mandate id and bundle run root in the workspace is a sha256 over the bytes
//! this crate emits. If those bytes diverge from RFC 8785 on any edge case, the digests are
//! irreproducible by any other implementation — and reproducibility by another implementation is
//! the entire property independent verification rests on. A divergence would not fail any
//! existing test: the reference producer and the reference verifier would simply agree with each
//! other, which is exactly the failure this corpus exists to make visible.
//!
//! `serde_jcs` is pinned to an exact version in `Cargo.toml` because its byte output moves those
//! digests. That pin protects against an unnoticed upgrade; it says nothing about whether the
//! pinned version is correct. This does.
//!
//! Expected bytes were cross-validated against an independent implementation in another language,
//! built from the primitives RFC 8785 defers to (see `_about.expected_provenance` in the vector
//! file). When adding a vector, derive its expectation from the RFC or from a second conforming
//! implementation — never from this crate, or the check becomes a snapshot of whatever we already
//! do and stops being a conformance check at all.
//!
//! **To confirm this corpus still bites**, swap `serde_jcs::to_vec` for `serde_json::to_vec` in
//! `src/jcs.rs` and run this file. At least 8 vectors must fail, and
//! `keyorder_utf16_vs_codepoint` must be among them — `serde_json::Map` also sorts, so it is the
//! only ordering vector here where code-unit, code-point and byte order disagree. A corpus that
//! survives that substitution is measuring nothing, and the property is written here rather than
//! left in a pull request description so it stays checkable rather than remembered.

use std::collections::BTreeMap;

#[derive(serde::Deserialize)]
struct Vector {
    input: String,
    expected: String,
    #[allow(dead_code)]
    pins: String,
}

fn vectors() -> BTreeMap<String, Vector> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vectors/rfc8785.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let mut all: BTreeMap<String, serde_json::Value> =
        serde_json::from_str(&raw).expect("vector file is not valid JSON");
    all.remove("_about");

    all.into_iter()
        .map(|(name, v)| {
            let vector: Vector = serde_json::from_value(v)
                .unwrap_or_else(|e| panic!("vector `{name}` has the wrong shape: {e}"));
            (name, vector)
        })
        .collect()
}

#[test]
fn canonical_bytes_match_rfc8785_for_every_vector() {
    let vectors = vectors();
    assert!(
        vectors.len() >= 31,
        "the corpus shrank to {} vectors; removing coverage here is a silent loss of the \
         cross-implementation guarantee",
        vectors.len()
    );

    let mut failures = Vec::new();
    for (name, v) in &vectors {
        let value: serde_json::Value = match serde_json::from_str(&v.input) {
            Ok(value) => value,
            Err(e) => {
                failures.push(format!("[{name}] input is not parseable JSON: {e}"));
                continue;
            }
        };
        match assay_canonical::jcs::to_vec(&value) {
            Err(e) => failures.push(format!("[{name}] canonicalization failed: {e}")),
            Ok(bytes) => {
                let got = String::from_utf8(bytes).expect("JCS output must be UTF-8");
                if got != v.expected {
                    failures.push(format!(
                        "[{name}]\n     got: {got:?}\n  wanted: {:?}\n    pins: {}",
                        v.expected, v.pins
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} RFC 8785 vectors diverged:\n\n{}\n\nA divergence here means digests produced by \
         this crate cannot be reproduced by a conforming implementation in another language.",
        failures.len(),
        vectors.len(),
        failures.join("\n\n")
    );
}

/// Canonicalization must be a function of the value, not of the text it was parsed from. Two
/// different spellings of the same JSON value have to produce identical bytes, or a producer and
/// a verifier that formatted their input differently would compute different content ids for the
/// same data.
#[test]
fn distinct_spellings_of_one_value_canonicalize_identically() {
    for (case, a, b) in [
        ("trailing zero", r#"{"a":4.50}"#, r#"{"a":4.5}"#),
        ("integral float", r#"{"a":1.0}"#, r#"{"a":1}"#),
        ("exponent form", r#"{"a":2e-3}"#, r#"{"a":0.002}"#),
        ("signed zero", r#"{"a":-0}"#, r#"{"a":0}"#),
        ("key order", r#"{"b":1,"a":2}"#, r#"{"a":2,"b":1}"#),
        ("whitespace", "{\"a\": 1,\n \"b\": 2}", r#"{"a":1,"b":2}"#),
        // `9007199254740993` vs `...992` deliberately does NOT belong here. Those are two
        // different integers that happen to round to one double, not one value spelled two ways,
        // and the collapse is precision loss rather than canonicalization. It is covered by
        // `integers_canonicalize_identically_from_a_struct_and_from_a_parsed_value`, where the
        // framing is right and a future divergence would send the reader to the correct cause.
    ] {
        let canon = |src: &str| {
            let v: serde_json::Value = serde_json::from_str(src).expect("parse");
            String::from_utf8(assay_canonical::jcs::to_vec(&v).expect("canonicalize"))
                .expect("utf8")
        };
        assert_eq!(canon(a), canon(b), "{case}: {a} and {b} must agree");
    }
}

/// Production values reach the canonicalizer as Rust structs, not as parsed `Value`s, and a `u64`
/// takes a different serde route in each case. RFC 8785 numbers are doubles, so a `u64` above
/// 2^53 loses precision either way — but the two routes must lose it *identically*, or the same
/// data would produce different digests depending only on how it was constructed.
///
/// Today no canonicalized field carries an integer near that boundary (the `u64`s in the evidence
/// types are counters and sequence numbers). This test is what makes that a bounded fact rather
/// than a hope: if a future field does cross it, the loss is at least consistent.
#[test]
fn integers_canonicalize_identically_from_a_struct_and_from_a_parsed_value() {
    #[derive(serde::Serialize)]
    struct Wrapper {
        a: u64,
    }

    for n in [
        42u64,
        9_007_199_254_740_992, // 2^53, the last exactly-representable integer
        9_007_199_254_740_993, // one past it
        18_446_744_073_709_551_615, // u64::MAX
    ] {
        let from_struct = String::from_utf8(
            assay_canonical::jcs::to_vec(&Wrapper { a: n }).expect("canonicalize"),
        )
        .expect("utf8");
        let value: serde_json::Value =
            serde_json::from_str(&format!("{{\"a\":{n}}}")).expect("parse");
        let from_value =
            String::from_utf8(assay_canonical::jcs::to_vec(&value).expect("canonicalize"))
                .expect("utf8");

        assert_eq!(
            from_struct, from_value,
            "{n} canonicalizes differently depending on whether it arrived as a Rust integer or \
             as parsed JSON, so the digest would depend on the construction path"
        );
    }
}

/// The two properties most likely to be broken by swapping the canonicalizer, stated as their own
/// test so a failure names the cause rather than appearing as one row in a table.
#[test]
fn the_two_properties_a_replacement_would_break() {
    let canon = |src: &str| {
        let v: serde_json::Value = serde_json::from_str(src).expect("parse");
        String::from_utf8(assay_canonical::jcs::to_vec(&v).expect("canonicalize")).expect("utf8")
    };

    // UTF-16 code-unit ordering. A non-BMP key begins with a leading surrogate (0xD800), which is
    // below U+E000 in UTF-16 and above it in both code-point and UTF-8 byte order. Every other
    // ordering vector in the corpus passes under all three rules; only this one separates them.
    let ordered = canon(r#"{"𐀀":1,"":2}"#);
    assert!(
        ordered.starts_with("{\"\u{10000}\""),
        "keys are not ordered by UTF-16 code unit — the non-BMP key must come first, got {ordered:?}"
    );

    // No Unicode normalization. If a canonicalizer applied NFC, these two keys would collapse into
    // one and the object would silently lose a member.
    let unnormalized = canon("{\"\u{e9}\":1,\"e\u{301}\":2}");
    assert!(
        unnormalized.matches(":").count() == 2,
        "a key was lost — the canonicalizer appears to apply Unicode normalization, which RFC 8785 \
         does not: {unnormalized:?}"
    );
}
