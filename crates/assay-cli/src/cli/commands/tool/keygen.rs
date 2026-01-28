//! `assay tool keygen` - Generate ed25519 keypair for signing.

use anyhow::{Context, Result};
use clap::Args;
use ed25519_dalek::SigningKey;
use std::fs;
use std::path::PathBuf;

use assay_core::mcp::signing::compute_key_id_from_verifying_key;

#[derive(Args, Debug)]
pub struct KeygenArgs {
    /// Output directory for keypair files
    #[arg(long, default_value = ".")]
    pub out: PathBuf,

    /// Force overwrite existing files
    #[arg(long, short)]
    pub force: bool,
}

pub fn cmd_keygen(args: KeygenArgs) -> i32 {
    match run_keygen(args) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e:#}");
            1
        }
    }
}

fn run_keygen(args: KeygenArgs) -> Result<()> {
    use pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};

    // Ensure output directory exists
    if !args.out.exists() {
        fs::create_dir_all(&args.out)
            .with_context(|| format!("failed to create directory: {}", args.out.display()))?;
    }

    let private_path = args.out.join("private_key.pem");
    let public_path = args.out.join("public_key.pem");

    // Check for existing files
    if !args.force {
        if private_path.exists() {
            anyhow::bail!(
                "private key already exists: {} (use --force to overwrite)",
                private_path.display()
            );
        }
        if public_path.exists() {
            anyhow::bail!(
                "public key already exists: {} (use --force to overwrite)",
                public_path.display()
            );
        }
    }

    // Generate keypair
    let signing_key = SigningKey::generate(&mut rand::thread_rng());
    let verifying_key = signing_key.verifying_key();

    // Encode as PEM
    let private_pem = signing_key
        .to_pkcs8_pem(LineEnding::LF)
        .context("failed to encode private key as PKCS#8 PEM")?;

    let public_pem = verifying_key
        .to_public_key_pem(LineEnding::LF)
        .context("failed to encode public key as SPKI PEM")?;

    // Write private key with restricted permissions
    fs::write(&private_path, private_pem.as_bytes())
        .with_context(|| format!("failed to write private key: {}", private_path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&private_path, perms)
            .with_context(|| format!("failed to set permissions on: {}", private_path.display()))?;
    }

    // Write public key
    fs::write(&public_path, public_pem)
        .with_context(|| format!("failed to write public key: {}", public_path.display()))?;

    // Compute and display key_id
    let key_id = compute_key_id_from_verifying_key(&verifying_key)?;

    println!("Generated ed25519 keypair:");
    println!(
        "  Private key: {} (PKCS#8 PEM, mode 0600)",
        private_path.display()
    );
    println!("  Public key:  {} (SPKI PEM)", public_path.display());
    println!();
    println!("key_id: {key_id}");
    println!();
    println!("Add this key_id to your trust policy to trust signatures from this key.");

    Ok(())
}
