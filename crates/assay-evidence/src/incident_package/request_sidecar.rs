//! Shape and byte-integrity checks for retained sidecar context, never join evidence.
use super::{is_valid_hex64, ResolveRefError};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub(super) const FORMAT: &str = "assay-request-receipts-ndjson-v0";
const COMMON: &[&str] = &[
    "schema",
    "assertion_basis",
    "scope",
    "sequence",
    "request_line",
    "request_bytes",
    "request_sha256",
    "rpc_id_present",
    "rpc_id",
];
const RECEIPT: &[&str] = &[
    "decision_event_id",
    "decision_sha256",
    "metadata_pointer",
    "declared_protocol_version_state",
    "declared_protocol_version",
];
const OBSERVATION: &[&str] = &["kind", "call_link", "call_id"];

#[derive(Default)]
pub(super) struct State {
    scope: Option<String>,
    sequence: u64,
}

impl State {
    /// The caller has already enforced strict JSON and all selected/hard budgets.
    pub(super) fn validate(&mut self, value: &Value) -> Result<(), ResolveRefError> {
        let invalid = || ResolveRefError::InputShape;
        let object = value.as_object().ok_or_else(invalid)?;
        let schema = value["schema"].as_str().ok_or_else(invalid)?;
        let (extra, basis) = match schema {
            "assay.request_decision_receipt.v0" => (RECEIPT, "caller_asserted"),
            "assay.request_observation.v0" => (OBSERVATION, "observed_on_wire"),
            _ => return Err(ResolveRefError::UnknownSchema),
        };
        if object.len() != COMMON.len() + extra.len()
            || COMMON
                .iter()
                .chain(extra)
                .any(|key| !object.contains_key(*key))
            || value["assertion_basis"] != basis
            || value["sequence"].as_u64() != Some(self.sequence)
        {
            return Err(invalid());
        }
        let scope = value["scope"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(invalid)?;
        if self.scope.as_deref().is_some_and(|prior| prior != scope) {
            return Err(invalid());
        }
        let line = value["request_line"].as_str().ok_or_else(invalid)?;
        if value["request_bytes"].as_u64() != Some(line.len() as u64)
            || value["request_sha256"].as_str()
                != Some(hex::encode(Sha256::digest(line.as_bytes())).as_str())
        {
            return Err(invalid());
        }
        let present = value["rpc_id_present"].as_bool().ok_or_else(invalid)?;
        if !present && !value["rpc_id"].is_null() {
            return Err(invalid());
        }
        if schema == "assay.request_decision_receipt.v0" {
            if value["decision_event_id"]
                .as_str()
                .is_none_or(str::is_empty)
                || !value["decision_sha256"]
                    .as_str()
                    .is_some_and(is_valid_hex64)
                || value["metadata_pointer"] != "/params/_meta"
            {
                return Err(invalid());
            }
            match value["declared_protocol_version_state"].as_str() {
                Some("present") if value["declared_protocol_version"].is_string() => {}
                Some("absent" | "malformed") if value["declared_protocol_version"].is_null() => {}
                _ => return Err(invalid()),
            }
        } else {
            if !matches!(value["kind"].as_str(), Some("ask" | "answer")) {
                return Err(invalid());
            }
            match value["call_link"].as_str() {
                Some("unique") if !value["call_id"].is_null() => {}
                Some("ambiguous" | "unlinked") if value["call_id"].is_null() => {}
                _ => return Err(invalid()),
            }
        }
        self.scope = Some(scope.to_owned());
        self.sequence = self.sequence.checked_add(1).ok_or_else(invalid)?;
        Ok(())
    }
}
