//! Tool signing CLI commands.
//!
//! Commands for managing MCP tool signatures.

pub mod keygen;
pub mod sign;
pub mod verify;

use clap::Subcommand;

fn read_tool_definition(path: &std::path::Path) -> anyhow::Result<serde_json::Value> {
    use anyhow::Context;
    use assay_common::limits::{LimitKind, LimitReader};
    use std::io::Read;

    let file = std::fs::File::open(path)
        .with_context(|| format!("failed to read tool file: {}", path.display()))?;
    let mut text = String::new();
    LimitReader::new(
        file,
        assay_common::trust_document::MAX_BYTES,
        LimitKind::SourceBytes,
    )
    .read_to_string(&mut text)
    .with_context(|| format!("failed to read tool file: {}", path.display()))?;
    validate_tool_definition(&text, path)?;
    Ok(serde_json::from_str(&text)?)
}

fn validate_tool_definition(text: &str, path: &std::path::Path) -> anyhow::Result<()> {
    assay_evidence::json_strict::validate_json_strict(text)
        .map_err(|_| anyhow::anyhow!("invalid tool JSON: {}", path.display()))
}

#[derive(Subcommand, Debug)]
pub enum ToolCmd {
    /// Generate ed25519 keypair for signing
    Keygen(keygen::KeygenArgs),

    /// Sign a tool definition
    Sign(sign::SignArgs),

    /// Verify a signed tool definition
    Verify(verify::VerifyArgs),
}

pub fn cmd_tool(cmd: ToolCmd) -> i32 {
    match cmd {
        ToolCmd::Keygen(args) => keygen::cmd_keygen(args),
        ToolCmd::Sign(args) => sign::cmd_sign(args),
        ToolCmd::Verify(args) => verify::cmd_verify(args),
    }
}
