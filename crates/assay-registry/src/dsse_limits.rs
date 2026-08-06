//! One ceiling for DSSE payloads, applied where the allocation happens.
//!
//! `assay-registry` base64-decodes and JSON-parses DSSE payloads in three flows -- pack signature
//! verification, sign-off, and SLSA provenance. Every one of them allocated the decoded payload
//! before anything looked at its size.
//!
//! PAE is the wrong place to fix that, and #1969 exists to say so before someone puts it there.
//! `build_pae` runs *after* the decode: by the time it is reached the memory is already committed,
//! so a ceiling in PAE would refuse a payload the process had finished allocating. The check has to
//! precede the decode, and the only quantity available then is the encoded length.
//!
//! The ceiling is domain-local on purpose. `assay-common` holds the DSSE Pre-Authentication
//! Encoding because two constructions of PAE are two definitions of what a signature covers; a size
//! budget is not that. It is this crate's answer about this crate's inputs, in the same class as
//! `VerifyLimits` describing an evidence bundle, and it does not travel.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;

/// The largest DSSE payload this crate will decode, in decoded bytes.
///
/// These payloads are in-toto Statements: a subject digest and a predicate. The largest thing that
/// legitimately appears is an SLSA provenance predicate listing resolved dependencies, which runs to
/// tens of kilobytes. 1 MiB is roughly two orders of magnitude above that -- generous enough that no
/// honest producer meets it, small enough that a hostile one cannot make the process allocate on
/// demand.
pub(crate) const MAX_DSSE_PAYLOAD_BYTES: usize = 1024 * 1024;

/// The refusal message for a surface whose reason is `&'static str` and cannot carry the numbers.
///
/// The compile-time assertion below is what keeps the wording and the constant from drifting: a
/// message naming a ceiling the code does not enforce is worse than one naming no ceiling at all.
pub(crate) const OVERSIZED_REASON: &str = "DSSE payload exceeds the 1 MiB ceiling";

const _: () = assert!(
    MAX_DSSE_PAYLOAD_BYTES == 1024 * 1024,
    "OVERSIZED_REASON says 1 MiB; change both or neither"
);

/// A payload refused for its size, carrying both numbers so the caller can say which ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PayloadTooLarge {
    pub(crate) limit: usize,
    /// Decoded bytes when known, otherwise the bound implied by the encoded length. `exact` says
    /// which, so a diagnostic never presents an upper bound as a measurement.
    pub(crate) size: usize,
    pub(crate) exact: bool,
}

impl PayloadTooLarge {
    pub(crate) fn reason(&self) -> String {
        if self.exact {
            format!(
                "DSSE payload is {} bytes, over the {}-byte ceiling",
                self.size, self.limit
            )
        } else {
            format!(
                "DSSE payload encodes to more than {} bytes, over the {}-byte ceiling",
                self.size, self.limit
            )
        }
    }
}

/// The largest base64 input that can decode within `MAX_DSSE_PAYLOAD_BYTES`.
///
/// Standard base64 emits four characters per three input bytes, padded up. Anything longer than
/// this decodes to more than the ceiling, so it can be refused without being decoded.
const fn max_encoded_len() -> usize {
    MAX_DSSE_PAYLOAD_BYTES.div_ceil(3) * 4
}

/// Base64-decode a DSSE payload, refusing an oversized one before it is allocated.
///
/// Two checks, and both are load-bearing. The first is on the encoded length and is what actually
/// bounds the allocation. The second is on the decoded length and is what makes the ceiling exact:
/// the encoded bound is conservative by up to three bytes of padding, and a limit that is
/// "1 MiB, give or take" is not a limit anyone can test.
///
/// `Ok(None)` means the input is not valid base64. The caller decides what that is, because the
/// three flows call it three things.
pub(crate) fn decode_bounded(payload_b64: &[u8]) -> Result<Option<Vec<u8>>, PayloadTooLarge> {
    if payload_b64.len() > max_encoded_len() {
        return Err(PayloadTooLarge {
            limit: MAX_DSSE_PAYLOAD_BYTES,
            size: MAX_DSSE_PAYLOAD_BYTES,
            exact: false,
        });
    }
    let decoded = match BASE64.decode(payload_b64) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(None),
    };
    if decoded.len() > MAX_DSSE_PAYLOAD_BYTES {
        return Err(PayloadTooLarge {
            limit: MAX_DSSE_PAYLOAD_BYTES,
            size: decoded.len(),
            exact: true,
        });
    }
    Ok(Some(decoded))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded_of(decoded_len: usize) -> String {
        BASE64.encode(vec![b'a'; decoded_len])
    }

    #[test]
    fn a_payload_at_the_ceiling_is_accepted() {
        let b64 = encoded_of(MAX_DSSE_PAYLOAD_BYTES);
        let decoded = decode_bounded(b64.as_bytes())
            .expect("exactly at the ceiling must not be refused")
            .expect("valid base64");
        assert_eq!(decoded.len(), MAX_DSSE_PAYLOAD_BYTES);
    }

    #[test]
    fn one_byte_over_the_ceiling_is_refused() {
        let b64 = encoded_of(MAX_DSSE_PAYLOAD_BYTES + 1);
        let err = decode_bounded(b64.as_bytes()).expect_err("one byte over must be refused");
        assert_eq!(err.limit, MAX_DSSE_PAYLOAD_BYTES);
    }

    /// The check that matters: refusal happens on the encoded length, before any decode.
    ///
    /// Without this the guard would still reject, but only after allocating the very thing the
    /// ceiling exists to prevent — which is the defect #1969 describes, moved rather than fixed.
    #[test]
    fn a_grossly_oversized_payload_is_refused_before_it_is_decoded() {
        let encoded_len = max_encoded_len() + 1;
        let b64 = vec![b'A'; encoded_len];
        let err = decode_bounded(&b64).expect_err("must be refused");
        assert!(
            !err.exact,
            "an inexact size means the refusal came from the encoded length, before the decode; \
             an exact one means the payload was allocated first"
        );
        assert_eq!(err.size, MAX_DSSE_PAYLOAD_BYTES);
    }

    #[test]
    fn the_encoded_bound_admits_everything_under_the_ceiling() {
        // The conservative direction has to be the safe one: the encoded bound must never refuse a
        // payload that would have decoded within the ceiling, or the exact check below it is dead.
        assert!(encoded_of(MAX_DSSE_PAYLOAD_BYTES).len() <= max_encoded_len());
        assert!(encoded_of(MAX_DSSE_PAYLOAD_BYTES - 1).len() <= max_encoded_len());
        assert!(encoded_of(MAX_DSSE_PAYLOAD_BYTES - 2).len() <= max_encoded_len());
    }

    #[test]
    fn invalid_base64_is_not_a_size_refusal() {
        assert_eq!(decode_bounded(b"!!!not base64!!!"), Ok(None));
    }

    #[test]
    fn an_ordinary_payload_round_trips() {
        let b64 = BASE64.encode(br#"{"_type":"https://in-toto.io/Statement/v1"}"#);
        let decoded = decode_bounded(b64.as_bytes()).unwrap().unwrap();
        assert_eq!(&decoded[..7], br#"{"_type"#);
    }

    #[test]
    fn a_refusal_never_presents_a_bound_as_a_measurement() {
        let inexact = PayloadTooLarge {
            limit: 10,
            size: 10,
            exact: false,
        };
        assert!(inexact.reason().contains("encodes to more than"));
        let exact = PayloadTooLarge {
            limit: 10,
            size: 11,
            exact: true,
        };
        assert!(exact.reason().contains("is 11 bytes"));
    }
}
