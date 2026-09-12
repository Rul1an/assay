//! Delegated-exec / child-tool observation carrier.
//!
//! This record is an observation of a child-tool invocation. It is not a policy
//! decision, not an outcome pairing, and not a privileged-mcp-action/v0 record.

use crate::crypto::id::compute_content_hash;
use crate::types::EvidenceEvent;
use anyhow::Result;
use serde_json::json;
use sha2::{Digest, Sha256};

/// Schema identity outside the privileged-mcp-action/v0 namespaces.
pub const DELEGATED_EXEC_OBSERVATION_SCHEMA: &str = "assay.delegated_exec_observation.v0";

const NON_CLAIMS: &[&str] = &[
    "delegated exec observation only; policy decision lives in assay.enforcement_decision.v0",
    "does not pair this invocation into a whole-action verdict",
    "does not assert or verify the child outcome or any provider side effect",
    "absence of this event is not proof the child did not run",
];

/// Emit one content-addressed delegated-exec observation event.
pub fn delegated_exec_observation_event(
    source: impl Into<String>,
    run_id: impl Into<String>,
    seq: u64,
    child_tool: &str,
    argv: &[u8],
) -> Result<EvidenceEvent> {
    let payload = json!({
        "schema": DELEGATED_EXEC_OBSERVATION_SCHEMA,
        "invocation": {
            "tool_name": child_tool,
            "argv_digest": format!("sha256:{}", hex::encode(Sha256::digest(argv))),
        },
        "non_claims": NON_CLAIMS,
    });
    let mut event = EvidenceEvent::new(
        DELEGATED_EXEC_OBSERVATION_SCHEMA,
        source,
        run_id,
        seq,
        payload,
    );
    event.content_hash = Some(compute_content_hash(&event)?);
    Ok(event)
}
