//! DSSE Pre-Authentication Encoding, for the crates that share this one.
//!
//! std-only, like [`crate::limits`]: it allocates.
//!
//! PAE is what a DSSE signature is actually taken over, so two implementations
//! of it are two definitions of what a signature covers. Byte-identical copies
//! are where a drift starts rather than a defence against one: nothing fails if
//! one gains a space, renders a length differently, or handles a non-ASCII
//! payload type another way, until a signature made by one side stops verifying
//! on the other.
//!
//! Scope: this is the workspace's one *production* construction. `assay-evidence`'s
//! mandate signing, `assay-core`'s MCP signing, and `assay-registry` all call it. Getting here took two counting errors worth recording, since both were
//! claims about how many copies existed made without opening every file: the
//! first said the workspace had two and it had six, the second said
//! `assay-registry` had four and it had three plus a test.
//!
//! Two constructions are deliberately left standing, in `assay-registry`'s
//! `sigstore_bundle` tests and in `assay-cli`'s supply-chain conformance tests.
//! Each builds a PAE, signs it, and asserts the production verifier accepts the
//! result, so they are independent constructions rather than copies: if this
//! module drifted, those signatures would stop verifying and the tests would
//! fail. Sharing them would leave the tests signing with the same code they
//! verify with, and a bug here would agree with itself and pass. That is why
//! "one production construction" and "two left standing" are consistent rather
//! than contradictory: the tests are the instrument, not a duplicate of it.

use std::string::ToString;
use std::vec::Vec;

/// Build the DSSE Pre-Authentication Encoding.
///
/// ```text
/// PAE(type, payload) = "DSSEv1" SP LEN(type) SP type SP LEN(payload) SP payload
/// ```
///
/// Lengths are the byte lengths of the UTF-8 `payload_type` and of the raw
/// `payload`, rendered in decimal ASCII. The payload is appended verbatim: it is
/// never re-canonicalized here, because a caller that signs re-serialized bytes
/// signs something the verifier does not hold.
///
/// ```
/// # use assay_common::dsse::build_pae;
/// assert_eq!(
///     build_pae("application/vnd.in-toto+json", b"{}"),
///     b"DSSEv1 28 application/vnd.in-toto+json 2 {}".to_vec()
/// );
/// ```
pub fn build_pae(payload_type: &str, payload: &[u8]) -> Vec<u8> {
    let type_len = payload_type.len().to_string();
    let payload_len = payload.len().to_string();

    // Preallocated: the exact size is known, and PAE is built for every signature
    // and every verification. `dsse::dsse_pae` carried this before the collapse
    // and the shared one dropped it, which is the kind of regression a
    // consolidation is otherwise free of.
    let mut pae = Vec::with_capacity(
        7 + type_len.len() + 1 + payload_type.len() + 1 + payload_len.len() + 1 + payload.len(),
    );
    pae.extend_from_slice(b"DSSEv1 ");
    pae.extend_from_slice(type_len.as_bytes());
    pae.push(b' ');
    pae.extend_from_slice(payload_type.as_bytes());
    pae.push(b' ');
    pae.extend_from_slice(payload_len.as_bytes());
    pae.push(b' ');
    pae.extend_from_slice(payload);
    pae
}

#[cfg(test)]
mod tests {
    use super::build_pae;

    #[test]
    fn empty_payload_and_type_still_carry_their_separators() {
        assert_eq!(build_pae("", b""), b"DSSEv1 0  0 ".to_vec());
    }

    /// The lengths are byte lengths, not character counts. A multi-byte payload
    /// type is where the two diverge, and a verifier reading character counts
    /// would accept a different framing for the same bytes.
    #[test]
    fn lengths_count_bytes_rather_than_characters() {
        let ty = "café/json"; // 9 chars, 10 bytes
        assert_eq!(ty.chars().count(), 9);
        let pae = build_pae(ty, b"x");
        // The diagnostic must not decode: a fixed-width slice of a PAE carrying a
        // multi-byte payload type can land mid-codepoint, and a panicking failure
        // message hides the assertion it was written to explain.
        assert!(
            pae.starts_with(b"DSSEv1 10 "),
            "type length must be the byte length, got prefix {:?}",
            &pae[..pae.len().min(14)]
        );
    }

    /// A payload carrying the separator byte must not be able to restructure the
    /// encoding, which is the property the length prefixes exist for.
    #[test]
    fn payload_containing_spaces_does_not_reframe_the_encoding() {
        let pae = build_pae("t", b"a b c");
        assert_eq!(pae, b"DSSEv1 1 t 5 a b c".to_vec());
    }

    /// Raw bytes are appended verbatim, including bytes that are not valid UTF-8.
    #[test]
    fn payload_is_not_required_to_be_utf8() {
        let pae = build_pae("t", &[0xff, 0xfe]);
        assert_eq!(pae, b"DSSEv1 1 t 2 \xff\xfe".to_vec());
    }
}
