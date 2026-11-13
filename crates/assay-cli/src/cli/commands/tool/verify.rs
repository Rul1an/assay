//! `assay mcp tool verify` - Verify a signed tool definition.
//!
//! Exit codes: 0 a signature was verified, or the tool is unsigned and the trust policy does not
//! require one (`require_signed` is false, which is also its default); 2 the tool is not signed;
//! 3 the signing key is not trusted; 4 the signature does not verify; 1 anything else, including
//! a call with no key and no policy.

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

/// Said for a signed and an unsigned tool alike: without a key or a policy nothing can be verified.
const NO_VERIFICATION_BASIS: &str =
    "must specify --pubkey, --trust-policy, or --allow-embedded-key";

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

    // An unsigned tool. Exit 0 from this command means a signature was verified, or a trust
    // policy says unsigned tools are accepted; nothing else. A key on the command line is a
    // request to verify against it, so a tool with nothing to verify fails, as a tool whose
    // signature was removed must.
    if !is_signed(&tool) {
        if let Some(policy_path) = &args.trust_policy {
            let policy = TrustPolicy::from_file(policy_path)?;
            if policy.require_signed {
                return Err(VerifyError::NoSignature.into());
            }
            if !args.quiet {
                println!(
                    "Tool is not signed (no x-assay-sig field). Accepted: the trust policy does \
                     not require a signature. No signature was verified."
                );
            }
            return Ok(());
        }
        if args.pubkey.is_some() || args.allow_embedded_key {
            return Err(VerifyError::NoSignature.into());
        }
        anyhow::bail!("{NO_VERIFICATION_BASIS}");
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
        // Use embedded key (dev mode) - print security warning
        if !args.quiet {
            eprintln!("WARNING: Using embedded public key from signature.");
            eprintln!("         This provides NO security guarantee - anyone can sign and embed their own key!");
            eprintln!("         Use --pubkey or --trust-policy for production verification.");
            eprintln!();
        }

        let sig = extract_signature(&tool).ok_or(VerifyError::NoSignature)?;
        let pubkey_b64 = sig.public_key.ok_or_else(|| {
            anyhow::anyhow!("--allow-embedded-key specified but no public_key in signature")
        })?;

        use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
        use ed25519_dalek::pkcs8::DecodePublicKey;

        let pubkey_bytes = BASE64
            .decode(&pubkey_b64)
            .context("failed to decode embedded public key base64")?;

        ed25519_dalek::VerifyingKey::from_public_key_der(&pubkey_bytes)
            .context("failed to parse embedded public key")?
    } else {
        anyhow::bail!("{NO_VERIFICATION_BASIS}");
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
