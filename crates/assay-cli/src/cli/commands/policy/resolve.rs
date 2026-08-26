//! `assay policy resolve` — dump the policy this Assay version would load.
//!
//! Guardrail: one `McpPolicy::policy_digest()` call is the only digest authority.
//! The CLI does not call JCS itself.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use assay_common::limits::{LimitKind, LimitReader};
use assay_core::mcp::policy::McpPolicy;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::cli::args::{OutputFormat, PolicyResolveArgs};
use crate::exit_codes::EXIT_CONFIG_ERROR;
use crate::output_write::write_stdout_json;

pub const SCHEMA_RESOLVED_V0: &str = "assay.policy.resolved.v0";
/// Must stay equal to `POLICY_SNAPSHOT_CANONICALIZATION_JCS_MCP_POLICY`.
const CANONICALIZATION_PROFILE: &str = "jcs:mcp_policy";
const MAX_INPUT_BYTES: u64 = 1_000_000;

#[derive(Serialize)]
struct ResolvedDocument {
    schema: &'static str,
    canonicalization_profile: &'static str,
    assay_version: &'static str,
    input_sha256: String,
    policy_digest: String,
    policy: serde_json::Value,
}

fn fail_closed() -> anyhow::Result<i32> {
    Ok(EXIT_CONFIG_ERROR)
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, ()> {
    let file = File::open(path).map_err(|_| ())?;
    let mut reader = LimitReader::new(file, MAX_INPUT_BYTES, LimitKind::SourceBytes);
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf).map_err(|_| ())?;
    Ok(buf)
}

fn input_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

pub async fn run(args: PolicyResolveArgs) -> anyhow::Result<i32> {
    if args.format != OutputFormat::Json {
        return fail_closed();
    }
    if args.deny_deprecations {
        std::env::set_var("ASSAY_STRICT_DEPRECATIONS", "1");
    }
    let bytes = match read_bounded(&args.input) {
        Ok(bytes) => bytes,
        Err(()) => return fail_closed(),
    };
    let policy = match McpPolicy::from_slice(&bytes) {
        Ok(policy) => policy,
        Err(_) => return fail_closed(),
    };
    if policy.try_compile_all_schemas().is_err() {
        return fail_closed();
    }
    let Some(policy_digest) = policy.policy_digest() else {
        return fail_closed();
    };
    let policy_value = serde_json::to_value(&policy)?;
    let document = ResolvedDocument {
        schema: SCHEMA_RESOLVED_V0,
        canonicalization_profile: CANONICALIZATION_PROFILE,
        assay_version: env!("CARGO_PKG_VERSION"),
        input_sha256: input_sha256(&bytes),
        policy_digest,
        policy: policy_value,
    };
    let json = serde_json::to_string(&document)?;
    Ok(write_stdout_json(&json))
}
