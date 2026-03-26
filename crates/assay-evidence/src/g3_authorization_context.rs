//! G3 v1 authorization-context predicates shared by Trust Basis and pack rules (P2a MCP-001).
//!
//! Must stay aligned with `crates/assay-core/src/mcp/g3_auth_context.rs` normalization at emission.

use crate::types::EvidenceEvent;

/// Maximum stored length for `auth_issuer` after trim (drop if exceeded).
pub const G3_MAX_AUTH_ISSUER_BYTES: usize = 2048;

fn g3_looks_like_jws_compact(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 3 {
        return false;
    }
    let (h, p, sig) = (parts[0], parts[1], parts[2]);
    if h.len() < 4 || p.len() < 4 || sig.len() < 4 {
        return false;
    }
    if !h.starts_with("eyJ") {
        return false;
    }
    let is_b64url = |part: &str| {
        part.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    };
    is_b64url(h) && is_b64url(p) && is_b64url(sig)
}

fn g3_has_bearer_credential_prefix(s: &str) -> bool {
    let b = s.trim().as_bytes();
    b.len() >= 7 && b[..7].eq_ignore_ascii_case(b"bearer ")
}

fn g3_principal_field_satisfies_v1(p: &str) -> bool {
    let t = p.trim();
    if t.is_empty() {
        return false;
    }
    !g3_has_bearer_credential_prefix(t) && !g3_looks_like_jws_compact(t)
}

fn g3_auth_issuer_field_satisfies_v1(iss: &str) -> bool {
    let t = iss.trim();
    if t.is_empty() || t.len() > G3_MAX_AUTH_ISSUER_BYTES {
        return false;
    }
    !g3_has_bearer_credential_prefix(t) && !g3_looks_like_jws_compact(t)
}

/// True iff this `assay.tool.decision` payload satisfies G3 v1 (same logic as Trust Basis).
pub fn decision_event_satisfies_g3_authorization_context_visible(event: &EvidenceEvent) -> bool {
    if event.type_ != "assay.tool.decision" {
        return false;
    }
    let Some(p) = event.payload.get("principal").and_then(|v| v.as_str()) else {
        return false;
    };
    if !g3_principal_field_satisfies_v1(p) {
        return false;
    }
    let Some(scheme) = event
        .payload
        .get("auth_scheme")
        .and_then(|v| v.as_str())
        .map(str::trim)
    else {
        return false;
    };
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "oauth2" && scheme != "jwt_bearer" {
        return false;
    }
    let Some(iss) = event.payload.get("auth_issuer").and_then(|v| v.as_str()) else {
        return false;
    };
    g3_auth_issuer_field_satisfies_v1(iss)
}

/// True iff `authorization_context_visible` would be `verified` in Trust Basis for this bundle.
pub fn bundle_satisfies_g3_authorization_context_visible(events: &[EvidenceEvent]) -> bool {
    events
        .iter()
        .any(decision_event_satisfies_g3_authorization_context_visible)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::EvidenceEvent;
    use chrono::{TimeZone, Utc};
    use serde_json::json;

    fn make_event(
        type_: &str,
        run_id: &str,
        seq: u64,
        payload: serde_json::Value,
    ) -> EvidenceEvent {
        let mut event = EvidenceEvent::new(type_, "urn:assay:test:g3", run_id, seq, payload);
        event.time = Utc.timestamp_opt(1_700_000_000 + seq as i64, 0).unwrap();
        event
    }

    #[test]
    fn bundle_satisfies_matches_g3_good_vector() {
        let e = make_event(
            "assay.tool.decision",
            "r",
            0,
            json!({
                "tool": "t",
                "decision": "allow",
                "principal": "alice@example.com",
                "auth_scheme": "jwt_bearer",
                "auth_issuer": "https://issuer.example/"
            }),
        );
        assert!(decision_event_satisfies_g3_authorization_context_visible(
            &e
        ));
        assert!(bundle_satisfies_g3_authorization_context_visible(&[e]));
    }

    #[test]
    fn bundle_satisfies_false_when_only_principal() {
        let e = make_event(
            "assay.tool.decision",
            "r",
            0,
            json!({
                "principal": "user:alice",
                "decision": "allow",
            }),
        );
        assert!(!bundle_satisfies_g3_authorization_context_visible(&[e]));
    }
}
