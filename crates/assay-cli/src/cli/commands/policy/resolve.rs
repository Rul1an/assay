//! `assay policy resolve` — dump the policy this Assay version would load.
//!
//! Guardrail: one `McpPolicy::policy_digest()` call is the only digest authority.
//! The CLI does not call JCS itself.

use assay_core::mcp::decision::POLICY_SNAPSHOT_CANONICALIZATION_JCS_MCP_POLICY;
use serde::Serialize;

use crate::cli::args::{OutputFormat, PolicyResolveArgs};
use crate::exit_codes::EXIT_CONFIG_ERROR;
use crate::output_write::write_stdout_json;

pub const SCHEMA_RESOLVED_V0: &str = "assay.policy.resolved.v0";

#[derive(Serialize)]
struct ResolvedDocument {
    schema: &'static str,
    canonicalization_profile: &'static str,
    assay_version: &'static str,
    input_sha256: String,
    policy_digest: String,
    policy: serde_json::Value,
}

pub async fn run(args: PolicyResolveArgs) -> anyhow::Result<i32> {
    if args.format != OutputFormat::Json {
        return Ok(EXIT_CONFIG_ERROR);
    }
    let bytes = super::resolved::read_bounded(&args.input)
        .map_err(|error| super::classify_load_error(&args.input, error))?;
    let resolved = super::resolved::load_resolved(&bytes)
        .map_err(|error| super::classify_load_error(&args.input, error))?;
    let policy_value = serde_json::to_value(&resolved.policy)?;
    let document = ResolvedDocument {
        schema: SCHEMA_RESOLVED_V0,
        canonicalization_profile: POLICY_SNAPSHOT_CANONICALIZATION_JCS_MCP_POLICY,
        assay_version: env!("CARGO_PKG_VERSION"),
        input_sha256: resolved.input_sha256,
        policy_digest: resolved.policy_digest,
        policy: policy_value,
    };
    let json = serde_json::to_string(&document)?;
    Ok(write_stdout_json(&json))
}
