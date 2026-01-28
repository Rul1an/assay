//! `assay tool verify` - Verify a signed tool definition.

use anyhow::{Context, Result};
use clap::Args;
use std::fs;
use std::path::PathBuf;

use assay_core::mcp::signing::{extract_signature, is_signed, verify_tool, VerifyError};
use assay_core::mcp::trust_policy::{load_public_key_pem, TrustPolicy};

#[derive(Args, Debug)]
pub struct VerifyArgs {
    /// Signed tool definition file (JSON)
    pub tool: PathBuf,

    /// Public key file (SPKI PEM) - mutually exclusive with --trust-policy
    #[arg(long, conflicts_with = "trust_policy")]
    pub pubkey: Option<PathBuf>,

    /// Trust policy file (YAML)
    #[arg(long, conflicts_with = "pubkey")]
    pub trust_policy: Option<PathBuf>,

    /// Allow using embedded public key (dev/testing only)
    #[arg(long)]
    pub allow_embedded_key: bool,

    /// Quiet mode - only exit code, no output
    #[arg(long, short)]
    pub quiet: bool,
}

pub fn cmd_verify(args: VerifyArgs) -> i32 {
    match run_verify(&args) {
        Ok(()) => 0,
        Err(e) => {
            if !args.quiet {
                eprintln!("error: {e:#}");
            }
            // Extract exit code from VerifyError if available
            if let Some(verify_err) = e.downcast_ref::<VerifyError>() {
                verify_err.exit_code()
            } else {
                1
            }
        }
    }
}

fn run_verify(args: &VerifyArgs) -> Result<()> {
    // Load tool definition
    let tool_json = fs::read_to_string(&args.tool)
        .with_context(|| format!("failed to read tool file: {}", args.tool.display()))?;

    let tool: serde_json::Value = serde_json::from_str(&tool_json)
        .with_context(|| format!("failed to parse tool JSON: {}", args.tool.display()))?;

    // Check if signed
    if !is_signed(&tool) {
        // Load trust policy to check if signature is required
        if let Some(policy_path) = &args.trust_policy {
            let policy = TrustPolicy::from_file(policy_path)?;
            if policy.require_signed {
                return Err(VerifyError::NoSignature.into());
            }
        }
        if !args.quiet {
            println!("Tool is not signed (no x-assay-sig field)");
        }
        return Ok(());
    }

    // Get the public key for verification
    let verifying_key = if let Some(pubkey_path) = &args.pubkey {
        // Explicit public key
        load_public_key_pem(pubkey_path)?
    } else if let Some(policy_path) = &args.trust_policy {
        // Load from trust policy
        let policy = TrustPolicy::from_file(policy_path)?;
        let sig = extract_signature(&tool).ok_or(VerifyError::NoSignature)?;

        // Check if key is trusted
        if !policy.is_key_trusted(&sig.key_id) {
            return Err(VerifyError::KeyNotTrusted {
                key_id: sig.key_id.clone(),
            }
            .into());
        }

        // Try to load the key from policy
        let loaded = policy.load_keys()?;
        loaded
            .into_iter()
            .find(|k| k.key_id == sig.key_id)
            .map(|k| k.key)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "key_id {} is trusted but no public_key_path provided in policy",
                    sig.key_id
                )
            })?
    } else if args.allow_embedded_key {
        // Use embedded key (dev mode)
        let sig = extract_signature(&tool).ok_or(VerifyError::NoSignature)?;
        let pubkey_b64 = sig.public_key.ok_or_else(|| {
            anyhow::anyhow!("--allow-embedded-key specified but no public_key in signature")
        })?;

        use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
        use pkcs8::DecodePublicKey;

        let pubkey_bytes = BASE64
            .decode(&pubkey_b64)
            .context("failed to decode embedded public key base64")?;

        ed25519_dalek::VerifyingKey::from_public_key_der(&pubkey_bytes)
            .context("failed to parse embedded public key")?
    } else {
        anyhow::bail!("must specify --pubkey, --trust-policy, or --allow-embedded-key");
    };

    // Verify signature
    let result = verify_tool(&tool, &verifying_key)?;

    if !args.quiet {
        println!("Verification successful!");
        println!();
        println!("  key_id:    {}", result.key_id);
        println!("  signed_at: {}", result.signed_at);
    }

    Ok(())
}
