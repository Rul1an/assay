//! Tool signing CLI commands.
//!
//! Commands for managing MCP tool signatures.

pub mod keygen;
pub mod sign;
pub mod verify;

use clap::Subcommand;

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
