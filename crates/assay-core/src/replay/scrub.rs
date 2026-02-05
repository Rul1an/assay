//! Replay bundle content scrubbing (E9b).
//!
//! Deny-by-default: redact known secret/token patterns so bundles are safe to share.
//! Used when writing cassettes into the bundle and when verifying (scan for forbidden patterns).
//! See E9-REPLAY-BUNDLE-PLAN §2.5 (bundle verify), §8.4.4 (scrub policy).
//!
//! **Encoding:** All operations are **byte-based** (no UTF-8 requirement). Verify never
//! fail-opens on invalid UTF-8; scrub does not corrupt binary (non-matching bytes unchanged).
//!
//! **Redaction scope:** (1) Authorization header: line replaced by `Authorization: [REDACTED]`
//! (header name preserved for forensics). (2) Bearer token: value only → `Bearer [REDACTED]`.
//! (3) sk-* key: key substring → `[REDACTED]`. Pattern covers sk- and sk_ (e.g. sk_live_).

use lazy_static::lazy_static;
use regex::bytes::{NoExpand, Regex};

const REDACTED: &[u8] = b"[REDACTED]";
/// Whole auth line replaced so redacted content no longer matches AUTH_HEADER (verify passes).
const AUTH_REDACTED_LINE: &[u8] = b"[REDACTED]\n";
const BEARER_REDACTED: &[u8] = b"Bearer [REDACTED]";

lazy_static! {
    /// Authorization header line (case-insensitive); whole line sensitive.
    static ref AUTH_HEADER: Regex = Regex::new(r"(?mi)^\s*Authorization\s*:\s*.+$").unwrap();
    /// Bearer token; token part redacted.
    static ref BEARER_TOKEN: Regex = Regex::new(r"(?i)Bearer\s+\S+").unwrap();
    /// API key: sk- or sk_ followed by 20+ word chars (covers sk-proj-, sk_live_, OpenAI-style).
    static ref SK_KEY: Regex = Regex::new(r"sk[-_][A-Za-z0-9_-]{20,}").unwrap();
}

/// Redacts known secret patterns in `data`. Byte-based: no UTF-8 conversion; binary unchanged
/// where no pattern matches. Use when writing cassette/file content into the bundle.
pub fn scrub_content(data: &[u8]) -> Vec<u8> {
    let s = AUTH_HEADER.replace_all(data, NoExpand(AUTH_REDACTED_LINE));
    let s = BEARER_TOKEN.replace_all(&s, NoExpand(BEARER_REDACTED));
    let s = SK_KEY.replace_all(&s, NoExpand(REDACTED));
    s.into_owned()
}

/// Returns true if `data` contains any forbidden pattern. Byte-based: invalid UTF-8 is still
/// scanned (no fail-open). Used by bundle verify: hard fail for cassettes/ and files/, warn for outputs/.
pub fn contains_forbidden_patterns(data: &[u8]) -> bool {
    AUTH_HEADER.is_match(data) || BEARER_TOKEN.is_match(data) || SK_KEY.is_match(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrub_redacts_auth_header() {
        let raw = b"Content-Type: application/json\nAuthorization: Bearer sk-secret123\n\n{}";
        let out = scrub_content(raw);
        assert!(!out.windows(10).any(|w| w == b"sk-secret"));
        assert!(out.windows(REDACTED.len()).any(|w| w == REDACTED));
        // Whole auth line replaced so no "Authorization" in redacted output (safe to share)
        assert!(!out
            .windows(b"Authorization".len())
            .any(|w| w == b"Authorization"));
    }

    #[test]
    fn scrub_redacts_bearer_and_sk() {
        let raw = b"Bearer sk-proj-abc123def456";
        let out = scrub_content(raw);
        assert!(!out.windows(6).any(|w| w == b"sk-proj"));
        assert!(
            out.windows(BEARER_REDACTED.len())
                .any(|w| w == BEARER_REDACTED)
                || out.windows(REDACTED.len()).any(|w| w == REDACTED)
        );
    }

    #[test]
    fn scrub_redacts_sk_key() {
        let raw = b"api_key=sk-abcdefghij1234567890xyz";
        let out = scrub_content(raw);
        assert!(!out.windows(14).any(|w| w == b"sk-abcdefghij"));
        assert!(out.windows(REDACTED.len()).any(|w| w == REDACTED));
    }

    #[test]
    fn contains_forbidden_detects_auth_header() {
        assert!(contains_forbidden_patterns(b"Authorization: Bearer SECRET"));
        assert!(contains_forbidden_patterns(b"authorization: bearer x"));
        assert!(!contains_forbidden_patterns(
            b"Content-Type: application/json"
        ));
    }

    #[test]
    fn contains_forbidden_detects_sk_key() {
        assert!(contains_forbidden_patterns(
            b"sk-abcdefghij1234567890abcdefghij"
        ));
        assert!(contains_forbidden_patterns(b"sk_live_abcdefghij1234567890"));
        assert!(!contains_forbidden_patterns(b"sk- short"));
    }

    #[test]
    fn safe_content_unchanged() {
        let safe = b"{\"method\":\"GET\",\"url\":\"/api\"}";
        let out = scrub_content(safe);
        assert_eq!(&out[..], safe);
        assert!(!contains_forbidden_patterns(safe));
    }

    /// Invalid UTF-8 with ASCII secret still detected (no fail-open).
    #[test]
    fn contains_forbidden_patterns_detects_in_valid_utf8_with_ascii_secret() {
        let with_secret = b"Authorization: Bearer SECRET\xff\xfe";
        assert!(
            contains_forbidden_patterns(with_secret),
            "verify must not skip non-UTF8 content that contains ASCII secrets"
        );
    }

    /// Binary without pattern is unchanged by scrub.
    #[test]
    fn scrub_preserves_binary_without_pattern() {
        let binary = [0u8, 1, 2, 0xff, 0xfe, 100];
        let out = scrub_content(&binary);
        assert_eq!(&out[..], &binary[..]);
    }

    // === Regex bypass surface tests (eval-esser F3) ===

    /// sk-key at exact boundary: 20 chars after prefix → matched.
    #[test]
    fn contains_forbidden_detects_sk_key_at_min_length() {
        // sk- + exactly 20 word chars = minimum match
        let at_boundary = b"sk-abcdefghij1234567890";
        assert_eq!(at_boundary.len(), 23); // 3 prefix + 20
        assert!(
            contains_forbidden_patterns(at_boundary),
            "sk-key with exactly 20 chars after prefix must be detected"
        );
    }

    /// sk-key below boundary: 19 chars after prefix → NOT matched (too short for a real key).
    #[test]
    fn contains_forbidden_ignores_sk_key_below_min_length() {
        let below_boundary = b"sk-abcdefghij123456789";
        assert_eq!(below_boundary.len(), 22); // 3 prefix + 19
        assert!(
            !contains_forbidden_patterns(below_boundary),
            "sk-key with only 19 chars after prefix must NOT be detected"
        );
    }

    /// sk_live_ variant at boundary: sk_ + 20 chars → matched.
    #[test]
    fn contains_forbidden_detects_sk_underscore_at_min_length() {
        let key = b"sk_live_abcdefghij12";
        assert_eq!(key.len(), 20); // 3 prefix + 17 < 20 chars after sk_
        assert!(
            !contains_forbidden_patterns(key),
            "sk_ + 17 chars should not match (need 20 after prefix)"
        );
        // Now with enough chars
        let key = b"sk_live_abcdefghij1234567890";
        // 3 (sk_) + 24 chars after sk_ → matches
        assert!(contains_forbidden_patterns(key));
    }

    /// Null byte in the middle of an Authorization header: regex still scans.
    #[test]
    fn contains_forbidden_detects_auth_with_null_byte_before() {
        // Null byte before "Authorization" should not prevent detection
        let data = b"\x00Authorization: Bearer secret";
        assert!(
            contains_forbidden_patterns(data),
            "null byte before auth header must not prevent detection"
        );
    }

    /// Multiline header continuation (obs-fold): Authorization:\r\n<SP>Bearer sk-secret.
    /// Current regex is line-based, so continuation value is on a separate line.
    /// Document the boundary: the header line itself is detected, but continuation-only is not.
    #[test]
    fn contains_forbidden_detects_auth_line_even_with_continuation() {
        // Standard case: header line itself contains the value
        let standard = b"Authorization: Bearer sk-proj-abcdef0123456789abcd\r\n";
        assert!(contains_forbidden_patterns(standard));

        // obs-fold case: value only on continuation line (no value on header line)
        // This is an acknowledged limitation of line-based regex.
        let obs_fold = b"Authorization:\r\n Bearer sk-proj-abcdef0123456789abcd\r\n";
        // The Authorization line itself matches (it has ":" followed by ".+"),
        // but only because "\r\n Bearer..." is on a separate line.
        // The Bearer token on the continuation line IS detected by the BEARER_TOKEN regex.
        assert!(
            contains_forbidden_patterns(obs_fold),
            "Bearer token on continuation line must still be caught by BEARER_TOKEN regex"
        );
    }

    /// Bearer token alone on continuation line (no Authorization header context).
    #[test]
    fn contains_forbidden_detects_bearer_on_standalone_line() {
        let data = b"  Bearer sk-proj-abcdef0123456789abcd\r\n";
        assert!(
            contains_forbidden_patterns(data),
            "BEARER_TOKEN regex must catch standalone bearer tokens"
        );
    }

    /// Base64-encoded key is NOT detected (acknowledged limitation; documents boundary).
    #[test]
    fn contains_forbidden_does_not_detect_base64_encoded_key() {
        // base64("sk-proj-abc123def456ghij") ≈ "c2stcHJvai1hYmMxMjNkZWY0NTZnaGlq"
        // This is an acknowledged limitation: scrub operates on literal patterns only.
        let b64_key = b"api_key=c2stcHJvai1hYmMxMjNkZWY0NTZnaGlq";
        assert!(
            !contains_forbidden_patterns(b64_key),
            "base64-encoded keys are outside scrub scope (acknowledged limitation)"
        );
    }
}
