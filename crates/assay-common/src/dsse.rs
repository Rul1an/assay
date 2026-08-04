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
//! Scope, stated exactly because an earlier version of this comment claimed the
//! workspace had two copies and it has six. `assay-evidence`'s mandate signing
//! and `assay-core`'s MCP signing call this. `assay-registry` carries four more
//! of its own, in `dsse`, `sigstore_bundle`, `supply_chain::provenance` and
//! `verify_internal::dsse`, and they are not consolidated here: that crate has
//! no internal dependencies at all, so folding them in means giving a leaf crate
//! a new edge, which is an architecture decision rather than a refactor. This
//! module is one construction for the crates that already share it, not one for
//! the workspace.

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

    let mut pae = Vec::new();
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
