//! `assay tool sign` - Sign a tool definition.

use anyhow::{Context, Result};
use clap::Args;
use std::fs;
use std::path::PathBuf;

use assay_core::mcp::signing::{extract_signature, sign_tool};
use assay_core::mcp::trust_policy::load_private_key_pem;

#[derive(Args, Debug)]
pub struct SignArgs {
    /// Tool definition file (JSON)
    pub tool: PathBuf,

    /// Private key file (PKCS#8 PEM)
    #[arg(long, short)]
    pub key: PathBuf,

    /// Output file (required unless --in-place)
    #[arg(long, short)]
    pub out: Option<PathBuf>,

    /// Modify input file in place
    #[arg(long, conflicts_with = "out")]
    pub in_place: bool,

    /// Embed public key in signature (dev/testing only)
    #[arg(long)]
    pub embed_pubkey: bool,
}

pub fn cmd_sign(args: SignArgs) -> i32 {
    match run_sign(args) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e:#}");
            1
        }
    }
}

fn run_sign(args: SignArgs) -> Result<()> {
    // Validate output destination
    let output_path = if args.in_place {
        args.tool.clone()
    } else if let Some(out) = args.out {
        out
    } else {
        anyhow::bail!("must specify --out <PATH> or --in-place");
    };

    // Load private key
    let signing_key = load_private_key_pem(&args.key)?;

    // Load tool definition
    let tool_json = fs::read_to_string(&args.tool)
        .with_context(|| format!("failed to read tool file: {}", args.tool.display()))?;

    let tool: serde_json::Value = serde_json::from_str(&tool_json)
        .with_context(|| format!("failed to parse tool JSON: {}", args.tool.display()))?;

    // Sign
    let signed = sign_tool(&tool, &signing_key, args.embed_pubkey)?;

    // Extract signature for display
    let sig = extract_signature(&signed).expect("just signed");

    // Write output
    let output_json = serde_json::to_string_pretty(&signed)?;
    fs::write(&output_path, output_json)
        .with_context(|| format!("failed to write output: {}", output_path.display()))?;

    // Display result
    println!("Signed tool definition:");
    println!("  Input:  {}", args.tool.display());
    println!("  Output: {}", output_path.display());
    println!();
    println!("Signature:");
    println!("  key_id:         {}", sig.key_id);
    println!("  payload_digest: {}", sig.payload_digest);
    println!("  signed_at:      {}", sig.signed_at);
    if sig.public_key.is_some() {
        println!("  public_key:     (embedded)");
    }

    Ok(())
}
