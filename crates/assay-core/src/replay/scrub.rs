//! Replay bundle content scrubbing (E9b).
//!
//! Deny-by-default: redact known secret/token patterns so bundles are safe to share.
//! Used when writing cassettes into the bundle and when verifying (scan for forbidden patterns).
//! See E9-REPLAY-BUNDLE-PLAN §2.5 (bundle verify), §8.4.4 (scrub policy).
//!
//! **Encoding:** We operate on bytes; for [scrub_content] we use UTF-8 lossy decoding (invalid
//! UTF-8 → replacement char U+FFFD) then run regex. For [contains_forbidden_patterns] we only
//! scan valid UTF-8 (invalid → treated as no match). Binary cassettes may thus get lossy chars
//! and could theoretically match patterns; SOTA follow-up: scan only in known-text entries or
//! restrict to ASCII substring scan.
//!
//! **Redaction scope:** (1) Authorization header: **whole line** replaced by `[REDACTED]`.
//! (2) Bearer token: only the token part is replaced (`Bearer XXX` → `Bearer [REDACTED]`).
//! (3) sk-* key: the key substring is replaced by `[REDACTED]`.

use lazy_static::lazy_static;
use regex::Regex;
use std::borrow::Cow;

const REDACTED: &str = "[REDACTED]";

lazy_static! {
    /// Authorization header line (case-insensitive); whole line is sensitive.
    static ref AUTH_HEADER: Regex = Regex::new(r"(?mi)^\s*Authorization\s*:\s*.+$").unwrap();
    /// Bearer token (e.g. "Bearer sk-..."); token part redacted.
    static ref BEARER_TOKEN: Regex = Regex::new(r"(?i)Bearer\s+\S+").unwrap();
    /// OpenAI-style API key (sk- followed by alphanumeric).
    static ref SK_KEY: Regex = Regex::new(r"sk-[a-zA-Z0-9]{20,}").unwrap();
}

/// Redacts known secret patterns in `data`. Returns UTF-8 string with secrets replaced by [REDACTED].
/// Use when writing cassette/file content into the bundle. Invalid UTF-8 is replaced with replacement char.
pub fn scrub_content(data: &[u8]) -> Cow<'_, str> {
    let s = String::from_utf8_lossy(data);
    let s = AUTH_HEADER.replace_all(&s, REDACTED);
    let s = BEARER_TOKEN.replace_all(&s, "Bearer [REDACTED]");
    let s = SK_KEY.replace_all(&s, REDACTED);
    Cow::Owned(s.into_owned())
}

/// Returns true if `data` contains any forbidden pattern (Authorization header, Bearer token, sk-* key).
/// Used by bundle verify: hard fail for cassettes/ and files/, warn for outputs/.
pub fn contains_forbidden_patterns(data: &[u8]) -> bool {
    let s = match std::str::from_utf8(data) {
        Ok(valid) => valid,
        Err(_) => return false,
    };
    AUTH_HEADER.is_match(s) || BEARER_TOKEN.is_match(s) || SK_KEY.is_match(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrub_redacts_auth_header() {
        let raw = b"Content-Type: application/json\nAuthorization: Bearer sk-secret123\n\n{}";
        let out = scrub_content(raw);
        assert!(!out.contains("sk-secret"));
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn scrub_redacts_bearer_and_sk() {
        let raw = b"Bearer sk-proj-abc123def456";
        let out = scrub_content(raw);
        assert!(!out.contains("sk-proj"));
        assert!(out.contains("Bearer [REDACTED]") || out.contains(REDACTED));
    }

    #[test]
    fn scrub_redacts_sk_key() {
        let raw = b"api_key=sk-abcdefghij1234567890xyz";
        let out = scrub_content(raw);
        assert!(!out.contains("sk-abcdefghij"));
        assert!(out.contains(REDACTED));
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
        assert!(!contains_forbidden_patterns(b"sk- short"));
    }

    #[test]
    fn safe_content_unchanged() {
        let safe = b"{\"method\":\"GET\",\"url\":\"/api\"}";
        let out = scrub_content(safe);
        assert_eq!(out.as_ref(), std::str::from_utf8(safe).unwrap());
        assert!(!contains_forbidden_patterns(safe));
    }
}
