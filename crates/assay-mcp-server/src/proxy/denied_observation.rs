//! Caller-visible denial observation carrier.
//!
//! This is deliberately separate from `assay.enforcement_decision.v0`: it records that the proxy
//! answered the caller with a proxy-originated denial surface. It does not decide policy and does not
//! assert anything about the upstream side effect.

use assay_mcp_server::tool_decision::sanitize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const DENIED_CALL_OBSERVATION_SCHEMA: &str = "assay.denied_call_observation.v0";

pub fn denied_call_observation_record(
    tool_name: &str,
    target_digest: Option<&str>,
    error_code: i32,
    reason: &str,
    caller_visible_response_line: &str,
) -> Value {
    json!({
        "schema": DENIED_CALL_OBSERVATION_SCHEMA,
        "call": {
            "tool_name": sanitize(tool_name),
            "target_digest": target_digest,
        },
        "caller_visible_error": {
            "code": error_code,
            "origin": "assay-proxy",
            "reason": reason,
        },
        "caller_visible_response_digest": sha256_bytes(caller_visible_response_line.as_bytes()),
        "non_claims": [
            "caller-visible proxy denial observation only; policy decision lives in assay.enforcement_decision.v0",
            "does not assert or verify the upstream side effect",
            "does not assert maliciousness, safety, approval, or whole-action trust",
            "must not be read as a replacement for the bound enforcement decision record"
        ]
    })
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_is_observation_not_verdict() {
        let response = r#"{"jsonrpc":"2.0","id":9,"error":{"code":-32042,"message":"tools/call denied by enforcing proxy: credential_scope","data":{"origin":"assay-proxy","reason":"credential_scope"}}}"#;
        let rec = denied_call_observation_record(
            "github.add_deploy_key",
            Some("sha256:abc"),
            -32042,
            "credential_scope",
            response,
        );

        assert_eq!(rec["schema"], DENIED_CALL_OBSERVATION_SCHEMA);
        assert_eq!(rec["call"]["tool_name"], "github.add_deploy_key");
        assert_eq!(rec["call"]["target_digest"], "sha256:abc");
        assert_eq!(rec["caller_visible_error"]["origin"], "assay-proxy");
        assert!(rec["caller_visible_response_digest"]
            .as_str()
            .unwrap()
            .starts_with("sha256:"));
        assert!(rec.get("decision").is_none());
        assert!(rec.get("target").is_none());
        assert!(rec.get("arguments").is_none());
    }
}
