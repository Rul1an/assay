#![allow(deprecated)]
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

fn claim<'a>(claims: &'a [serde_json::Value], id: &str) -> &'a serde_json::Value {
    claims
        .iter()
        .find(|claim| claim["id"] == id)
        .expect("claim should exist")
}

#[path = "evidence_test/deterministic_and_guardrails.rs"]
mod deterministic_and_guardrails;
#[path = "evidence_test/flow_and_eval_receipts.rs"]
mod flow_and_eval_receipts;
#[path = "evidence_test/importer_receipts.rs"]
mod importer_receipts;
#[path = "evidence_test/mcp_execution_records.rs"]
mod mcp_execution_records;
#[path = "evidence_test/mcp_tunnel_observed.rs"]
mod mcp_tunnel_observed;
