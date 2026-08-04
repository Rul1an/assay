//! DSSE Pre-Authentication Encoding, once for this crate.
//!
//! PAE is what a DSSE signature is taken over, so a second construction of it is
//! a second answer to what a signature covers. This crate had three, in `dsse`,
//! `supply_chain::provenance` and `verify::verify_internal::dsse`, all
//! behaviourally identical. Identical is where a drift starts rather than a
//! defence against one: nothing fails if one gains a space, renders a length
//! differently, or handles a non-ASCII payload type another way, until a
//! signature made by one path stops verifying on another.
//!
//! Why here and not `assay_common::dsse`, which holds the same construction for
//! `assay-evidence` and `assay-core`: this crate has no internal dependencies at
//! all, and giving a leaf crate its first edge is an architecture decision
//! rather than a refactor. Sharing across that line is worth doing and is worth
//! deciding on its own terms; nothing about it needs to block collapsing three
//! copies into one inside a crate that already builds without it.

/// Build the DSSE Pre-Authentication Encoding.
///
/// ```text
/// PAE(type, payload) = "DSSEv1" SP LEN(type) SP type SP LEN(payload) SP payload
/// ```
///
/// Lengths are the byte lengths of the UTF-8 `payload_type` and of the raw
/// `payload`, in decimal ASCII. DSSE v1.0.0 defines the type input as a byte
/// sequence, `PAE(UTF8(PAYLOAD_TYPE), SERIALIZED_BODY)`; taking `&str` here is
/// deliberately stricter, since a payload type that is not valid UTF-8 cannot be
/// constructed rather than being encoded and rejected later.
///
/// The payload is appended verbatim and never re-serialized: a caller that signs
/// re-canonicalized bytes signs something the verifier does not hold.
pub(crate) fn build_pae(payload_type: &str, payload: &[u8]) -> Vec<u8> {
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

    /// The vector from the DSSE specification's own worked example shape: the
    /// lengths bracket each field, so nothing inside either can restructure the
    /// encoding.
    #[test]
    fn separators_are_bracketed_by_lengths() {
        assert_eq!(build_pae("t", b"a b c"), b"DSSEv1 1 t 5 a b c".to_vec());
    }

    /// Lengths are byte lengths, not character counts. A multi-byte payload type
    /// is where the two readings diverge, and this crate signs pack and bundle
    /// media types that are ASCII today and need not stay that way.
    #[test]
    fn lengths_count_bytes_rather_than_characters() {
        let ty = "café/json";
        assert_eq!(ty.chars().count(), 9);
        assert_eq!(ty.len(), 10);
        assert!(build_pae(ty, b"x").starts_with(b"DSSEv1 10 "));
    }

    #[test]
    fn payload_need_not_be_utf8() {
        assert_eq!(
            build_pae("t", &[0xff, 0xfe]),
            b"DSSEv1 1 t 2 \xff\xfe".to_vec()
        );
    }

    #[test]
    fn empty_type_and_payload_keep_their_separators() {
        assert_eq!(build_pae("", b""), b"DSSEv1 0  0 ".to_vec());
    }
}
