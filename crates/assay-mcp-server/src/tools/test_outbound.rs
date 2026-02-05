//! Test-only outbound hook for E6a.3 no-pass-through E2E test.
//! When ASSAY_TEST_OUTBOUND_URL is set, performs one GET using [crate::auth::build_downstream_headers]
//! only (no inbound auth forwarded). Single callsite for outbound header construction.
#![cfg(feature = "test-outbound")]

use crate::auth::build_downstream_headers;
use serde_json::Value;

pub async fn test_outbound(_args: &Value) -> anyhow::Result<Value> {
    let url = match std::env::var("ASSAY_TEST_OUTBOUND_URL") {
        Ok(u) if !u.is_empty() => u,
        _ => {
            return Ok(
                serde_json::json!({ "allowed": true, "skipped": "ASSAY_TEST_OUTBOUND_URL not set" }),
            )
        }
    };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()?;
    let mut req = client.get(&url);
    // SECURITY-INVARIANT: outbound headers only from build_downstream_headers(); never forward inbound.
    for (name, value) in build_downstream_headers() {
        req = req.header(name, value);
    }
    let resp = req.send().await?;
    let status = resp.status().as_u16();
    Ok(serde_json::json!({
        "allowed": true,
        "outbound_status": status,
        "outbound_headers_from_allowlist_only": true
    }))
}
