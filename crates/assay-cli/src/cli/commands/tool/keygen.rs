//! `assay mcp tool keygen` - Generate ed25519 keypair for signing.

use anyhow::{Context, Result};
use clap::Args;
use ed25519_dalek::SigningKey;
use getrandom::{rand_core::UnwrapErr, SysRng};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

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
    use ed25519_dalek::pkcs8::{spki::der::pem::LineEnding, EncodePrivateKey, EncodePublicKey};

    // Ensure output directory exists
    if !args.out.exists() {
        fs::create_dir_all(&args.out)
            .with_context(|| format!("failed to create directory: {}", args.out.display()))?;
    }

    let private_path = args.out.join("private_key.pem");
    let public_path = args.out.join("public_key.pem");

    // Reject links and special files before changing either destination.
    check_destination(&private_path, "private", args.force)?;
    check_destination(&public_path, "public", args.force)?;

    // Generate keypair
    let mut csprng = UnwrapErr(SysRng);
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();

    // Encode as PEM
    let private_pem = signing_key
        .to_pkcs8_pem(LineEnding::LF)
        .context("failed to encode private key as PKCS#8 PEM")?;

    let public_pem = verifying_key
        .to_public_key_pem(LineEnding::LF)
        .context("failed to encode public key as SPKI PEM")?;

    // A fresh inode keeps the new secret out of old hardlinks and open handles.
    // The output directory and its parents must be controlled by the caller.
    let mut private_file = tempfile::NamedTempFile::new_in(&args.out)
        .context("failed to create private key temporary file")?;
    #[cfg(unix)]
    let private_mode = {
        use std::os::unix::fs::PermissionsExt;
        let mode = private_file.as_file().metadata()?.permissions().mode() & 0o777;
        anyhow::ensure!(
            mode & 0o077 == 0,
            "private key permissions allow group or other access"
        );
        mode
    };
    private_file
        .write_all(private_pem.as_bytes())
        .with_context(|| format!("failed to write private key: {}", private_path.display()))?;
    if args.force {
        private_file.persist(&private_path)
    } else {
        private_file.persist_noclobber(&private_path)
    }
    .with_context(|| format!("failed to publish private key: {}", private_path.display()))?;

    // Write public key
    fs::write(&public_path, public_pem)
        .with_context(|| format!("failed to write public key: {}", public_path.display()))?;

    // Compute and display key_id
    let key_id = compute_key_id_from_verifying_key(&verifying_key)?;

    println!("Generated ed25519 keypair:");
    #[cfg(unix)]
    println!(
        "  Private key: {} (PKCS#8 PEM, mode {private_mode:04o})",
        private_path.display()
    );
    #[cfg(not(unix))]
    println!("  Private key: {} (PKCS#8 PEM)", private_path.display());
    println!("  Public key:  {} (SPKI PEM)", public_path.display());
    println!();
    println!("key_id: {key_id}");
    println!();
    println!("Add this key_id to your trust policy to trust signatures from this key.");

    Ok(())
}

fn check_destination(path: &Path, kind: &str, force: bool) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            anyhow::ensure!(
                metadata.is_file(),
                "refusing non-regular {kind} key destination: {}",
                path.display()
            );
            anyhow::ensure!(
                force,
                "{kind} key already exists: {} (use --force to overwrite)",
                path.display()
            );
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("failed to inspect {kind} key: {}", path.display()))
        }
    }
}
