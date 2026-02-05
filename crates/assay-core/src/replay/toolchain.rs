//! Toolchain and runner metadata capture for replay bundle (E9.2).
//!
//! Captures rustc, cargo, Cargo.lock hash, and runner (os, arch, container, CI).
//! All fields normative but "unknown" allowed when capture fails or is unavailable.

use crate::replay::manifest::{RunnerMeta, ToolchainMeta};
use sha2::{Digest, Sha256};
use std::process::Command;

/// Capture current toolchain metadata. Returns ToolchainMeta with "unknown" or
/// empty where capture fails (e.g. not in a cargo project).
pub fn capture_toolchain() -> ToolchainMeta {
    let rustc = Command::new("rustc")
        .args(["-Vv"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let cargo = Command::new("cargo")
        .arg("-V")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let cargo_lock_hash = std::path::Path::new("Cargo.lock")
        .exists()
        .then(|| std::fs::read("Cargo.lock").ok())
        .flatten()
        .map(|data| format!("sha256:{}", hex::encode(Sha256::digest(&data))));

    let runner = Some(RunnerMeta {
        os: Some(std::env::consts::OS.to_string()),
        arch: Some(std::env::consts::ARCH.to_string()),
        container_image_digest: None,
        ci: std::env::var_os("CI").is_some().then_some(true),
    });

    ToolchainMeta {
        rustc: rustc.or_else(|| Some("unknown".into())),
        cargo: cargo.or_else(|| Some("unknown".into())),
        cargo_lock_hash: cargo_lock_hash.or_else(|| Some("unknown".into())),
        runner,
    }
}
