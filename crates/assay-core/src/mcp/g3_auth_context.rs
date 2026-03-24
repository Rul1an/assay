//! G3 v1 — Authorization Context Evidence (policy-projected fields only).
//!
//! See `docs/architecture/PLAN-G3-AUTHORIZATION-CONTEXT-EVIDENCE-2026q2.md`.

use super::policy::PolicyMatchMetadata;

/// Allowlisted `auth_scheme` values (lowercase JSON).
pub const AUTH_SCHEME_OAUTH2: &str = "oauth2";
pub const AUTH_SCHEME_JWT_BEARER: &str = "jwt_bearer";

/// Maximum stored length for `auth_issuer` after trim (drop if exceeded).
const MAX_AUTH_ISSUER_BYTES: usize = 2048;

/// Rejects JWS compact strings (`header.payload.signature`) — not valid `iss` / principal (v1: no JWT dumps).
fn looks_like_jws_compact(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 3 {
        return false;
    }
    let (h, p, sig) = (parts[0], parts[1], parts[2]);
    if h.len() < 4 || p.len() < 4 || sig.len() < 4 {
        return false;
    }
    // Typical JWT header base64url begins with `{"` → `eyJ`
    if !h.starts_with("eyJ") {
        return false;
    }
    let is_b64url = |part: &str| {
        part.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    };
    is_b64url(h) && is_b64url(p) && is_b64url(sig)
}

fn has_bearer_credential_prefix(s: &str) -> bool {
    let b = s.trim().as_bytes();
    b.len() >= 7 && b[..7].eq_ignore_ascii_case(b"bearer ")
}

/// Optional projection merged into [`PolicyMatchMetadata`] after policy evaluation
/// on the supported MCP tool-call / decision path (tests and configured handlers).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuthContextProjection {
    pub auth_scheme: Option<String>,
    pub auth_issuer: Option<String>,
    pub principal: Option<String>,
}

impl AuthContextProjection {
    /// Normalize and assign G3 fields on metadata. Unknown `auth_scheme` values are dropped.
    pub fn merge_into_metadata(&self, metadata: &mut PolicyMatchMetadata) {
        if let Some(s) = normalize_auth_scheme(self.auth_scheme.as_deref()) {
            metadata.auth_scheme = Some(s);
        }
        if let Some(i) = normalize_auth_issuer(self.auth_issuer.as_deref()) {
            metadata.auth_issuer = Some(i);
        }
        if let Some(p) = normalize_principal(self.principal.as_deref()) {
            metadata.principal = Some(p);
        }
    }
}

/// Returns allowlisted scheme string or `None` if unknown / empty.
pub fn normalize_auth_scheme(input: Option<&str>) -> Option<String> {
    let s = input?.trim();
    if s.is_empty() {
        return None;
    }
    let lower = s.to_ascii_lowercase();
    match lower.as_str() {
        AUTH_SCHEME_OAUTH2 | AUTH_SCHEME_JWT_BEARER => Some(lower),
        _ => None,
    }
}

/// v1 norm: trimmed JWT `iss`-style string (no raw JWT dump). Oversized input dropped.
pub fn normalize_auth_issuer(input: Option<&str>) -> Option<String> {
    let s = input?.trim();
    if s.is_empty() {
        return None;
    }
    if has_bearer_credential_prefix(s) || looks_like_jws_compact(s) {
        return None;
    }
    if s.len() > MAX_AUTH_ISSUER_BYTES {
        return None;
    }
    Some(s.to_string())
}

/// Principal for G3: Unicode-trimmed; whitespace-only ⇒ absent.
pub fn normalize_principal(input: Option<&str>) -> Option<String> {
    let s = input.map(str::trim)?;
    if s.is_empty() {
        return None;
    }
    if has_bearer_credential_prefix(s) || looks_like_jws_compact(s) {
        return None;
    }
    Some(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheme_allowlist() {
        assert_eq!(
            normalize_auth_scheme(Some("JWT_BEARER")).as_deref(),
            Some(AUTH_SCHEME_JWT_BEARER)
        );
        assert_eq!(
            normalize_auth_scheme(Some("  oauth2  ")).as_deref(),
            Some("oauth2")
        );
        assert_eq!(normalize_auth_scheme(Some("openid")), None);
    }

    #[test]
    fn issuer_trim_and_cap() {
        assert_eq!(
            normalize_auth_issuer(Some("  https://issuer.example  ")).as_deref(),
            Some("https://issuer.example")
        );
        let huge = "x".repeat(MAX_AUTH_ISSUER_BYTES + 1);
        assert_eq!(normalize_auth_issuer(Some(&huge)), None);
    }

    #[test]
    fn principal_whitespace_absent() {
        assert_eq!(normalize_principal(Some("   \n\t  ")), None);
        assert_eq!(normalize_principal(Some("alice")).as_deref(), Some("alice"));
    }

    /// Synthetic JWS-shaped string (not a real credential; avoids well-known JWT literals in scanners).
    const SYNTHETIC_JWS_COMPACT: &str =
        "eyJxxxxxxxxxxxxxxxxxxxx.yyyyyyyyyyyyyyyyyyyyyyyy.zzzzzzzzzzzzzzzzzzzzzzzz";

    #[test]
    fn issuer_and_principal_reject_jws_compact_and_bearer_material() {
        assert_eq!(normalize_auth_issuer(Some(SYNTHETIC_JWS_COMPACT)), None);
        assert_eq!(normalize_auth_issuer(Some("Bearer secret-token")), None);
        assert_eq!(normalize_principal(Some(SYNTHETIC_JWS_COMPACT)), None);
        assert_eq!(normalize_principal(Some("Bearer opaque-credential")), None);
    }

    #[test]
    fn bearer_prefix_check_does_not_panic_on_non_ascii_leading_chars() {
        // Must not slice `str` at byte 7 — use byte prefix compare only.
        let s = "\u{00e9}Bearer token";
        assert!(!has_bearer_credential_prefix(s));
        assert!(has_bearer_credential_prefix("Bearer ok"));
    }
}
