//! No pass-through of inbound auth to downstream (E6a.3).
//!
//! **Security invariant:** Inbound authentication material MUST NOT be forwarded to any
//! downstream HTTP call (LLM/judge/proxy). Outbound requests MUST be built from an allowlist
//! of headers or pass through [strip_sensitive_headers] before send. Tests enforce this invariant.
//!
//! **PR packet (review/sign-off):**
//! 1. **Header set (denylist):** [SENSITIVE_HEADER_NAMES] — comparison is case-insensitive.
//! 2. **Outbound callsites:** (a) JWKS in [crate::auth::jwks] — no request-derived headers.
//!    (b) Test-only outbound in [crate::tools::test_outbound] — uses [build_downstream_headers] only.
//! 3. **E2E proof:** `tests/no_passthrough_e2e.rs` — inbound auth in initialize, then outbound call;
//!    assert mock received no sensitive header names (audit-grade failure message on leak).

use std::collections::HashSet;

/// Header names that must never be forwarded from inbound to downstream (case-insensitive).
/// Covers common credential/cookie leak paths (RFC and de-facto).
/// Includes response-style names (set-cookie, cookie2) defensively; some proxies misroute headers.
pub const SENSITIVE_HEADER_NAMES: &[&str] = &[
    "authorization",
    "x-api-key",
    "proxy-authorization",
    "cookie",
    "cookie2",
    "x-auth-token",
    "x-access-token",
    "x-forwarded-authorization",
    "set-cookie",
];

fn sensitive_set() -> HashSet<&'static str> {
    SENSITIVE_HEADER_NAMES.iter().copied().collect()
}

/// Returns true if the header name is sensitive (case-insensitive).
#[inline]
pub fn is_sensitive(name: &str) -> bool {
    sensitive_set().contains(name.to_lowercase().as_str())
}

/// Strip all sensitive headers from a map (case-insensitive name check).
/// Prefer [build_downstream_headers] (allowlist) for new code; use this only when
/// you must filter an existing set. Contract: downstream headers are never "forward inbound";
/// they are built from scratch or explicitly filtered.
pub fn strip_sensitive_headers<K, V>(headers: &[(K, V)]) -> Vec<(K, V)>
where
    K: AsRef<str> + Clone,
    V: Clone,
{
    let set = sensitive_set();
    headers
        .iter()
        .filter(|(k, _)| !set.contains(k.as_ref().to_lowercase().as_str()))
        .cloned()
        .collect()
}

/// Build headers for a downstream HTTP request from an allowlist only.
/// No inbound auth is ever included. Use this at every outbound call site.
#[inline]
pub fn build_downstream_headers() -> Vec<(&'static str, String)> {
    // Allowlist: only safe headers we explicitly add (e.g. user-agent, accept).
    // Empty by default; extend when we need content-type, traceparent, etc.
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_sensitive_case_insensitive() {
        assert!(is_sensitive("Authorization"));
        assert!(is_sensitive("authorization"));
        assert!(is_sensitive("AUTHORIZATION"));
        assert!(is_sensitive("x-Api-Key"));
        assert!(is_sensitive("X-Forwarded-Authorization"));
        assert!(is_sensitive("Cookie"));
        assert!(is_sensitive("set-cookie"));
        assert!(!is_sensitive("content-type"));
        assert!(!is_sensitive("accept"));
        assert!(!is_sensitive("user-agent"));
    }

    #[test]
    fn strip_removes_all_sensitive_names() {
        let headers: Vec<(String, String)> = vec![
            ("Authorization".into(), "Bearer INBOUND".into()),
            ("x-api-key".into(), "secret".into()),
            ("Content-Type".into(), "application/json".into()),
            ("cookie".into(), "session=INBOUND".into()),
            ("X-Auth-Token".into(), "token".into()),
            ("Accept".into(), "*/*".into()),
        ];
        let out = strip_sensitive_headers(&headers);
        let names: Vec<&str> = out.iter().map(|(k, _)| k.as_str()).collect();
        assert!(!names
            .iter()
            .any(|n| n.eq_ignore_ascii_case("authorization")));
        assert!(!names.iter().any(|n| n.eq_ignore_ascii_case("x-api-key")));
        assert!(!names.iter().any(|n| n.eq_ignore_ascii_case("cookie")));
        assert!(!names.iter().any(|n| n.eq_ignore_ascii_case("x-auth-token")));
        assert!(names.contains(&"Content-Type"));
        assert!(names.contains(&"Accept"));
    }

    #[test]
    fn build_downstream_headers_empty_by_default() {
        let h = build_downstream_headers();
        assert!(h.is_empty());
    }

    #[test]
    fn strip_removes_duplicate_sensitive_headers() {
        let headers: Vec<(String, String)> = vec![
            ("Authorization".into(), "Bearer A".into()),
            ("Content-Type".into(), "application/json".into()),
            ("authorization".into(), "Bearer B".into()),
        ];
        let out = strip_sensitive_headers(&headers);
        let names: Vec<&str> = out.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(names.len(), 1);
        assert_eq!(names[0], "Content-Type");
    }
}
