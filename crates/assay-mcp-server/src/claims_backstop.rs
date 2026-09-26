//! Test-only shared backstop for ADR-043 §2 wire status claims (#2232).
//!
//! One list, one meaning: every generated-surface guard calls
//! [`assert_no_unearned_status`], so adding a word to [`UNEARNED_STATUS_WORDS`]
//! covers the handshake, the fail-closed tool result, the SARIF projection, and
//! the policy decision contract at once. A second list would be a second
//! definition of an unearned claim, free to drift from this one.
//!
//! This file is compiled into both test builds — the library's via
//! `#[cfg(test)] mod claims_backstop;` in `lib.rs` and the binary's via the
//! same declaration in `main.rs` — so both targets call this same function over
//! these same words. The module is `#[cfg(test)]`-gated and private in both, so
//! sharing it adds no public API and no Cargo change.

/// ADR-043 §2's closed set of forbidden public wire status claims, in one place.
///
/// It lived inline in the `initialize` test, which made it the stop list for exactly one
/// response: adding a word covered that response and nothing else, and a second generated
/// surface could only be covered by writing a second list free to drift from this one
/// (#2232). One list, one meaning — adding a word here now covers every surface that calls
/// [`assert_no_unearned_status`].
pub(crate) const UNEARNED_STATUS_WORDS: [&str; 8] = [
    "certified",
    "certification",
    "partner",
    "compliant",
    "compliance",
    "approved",
    "endorsed",
    "accredited",
];

/// Assert that one Assay-originated generated response asserts no unearned status.
///
/// A denylist over the serialized value catches a claim reintroduced anywhere in the object
/// under any nesting, but it only knows the words it was given. It is a backstop, never the
/// primary control: a claim can still live in a *value* on a permitted path. Applying this to a
/// surface is therefore a floor, not a certificate that the surface is fully pinned.
pub(crate) fn assert_no_unearned_status(label: &str, value: &serde_json::Value) {
    let wire = serde_json::to_string(value).expect("serializable");
    let haystack = wire.to_ascii_lowercase();
    for forbidden in UNEARNED_STATUS_WORDS {
        assert!(
            !haystack.contains(forbidden),
            "{label} asserts `{forbidden}` without a checkable basis: {wire}"
        );
    }
}

/// The control for [`assert_no_unearned_status`]. A guard never shown to reject anything
/// proves nothing, and this one is a denylist, so the thing worth pinning is that it actually
/// fires — including on a word nested below the top level, which is the shape it exists for.
#[test]
#[should_panic(expected = "asserts `certified`")]
fn unearned_status_rule_rejects_a_nested_claim() {
    assert_no_unearned_status(
        "control",
        &serde_json::json!({"serverInfo": {"name": "assay", "status": "certified"}}),
    );
}
