//! Shared plumbing for tests in this crate that invoke Cargo as a subprocess.
//!
//! Two integration tests here shell out to Cargo — `e2e_mcp_wrap_assert_cmd`
//! builds `assay-mcp-server`, and `golden_runner` runs `assay`. They must do it
//! the same way, and not because duplication is untidy: when only one of them
//! strips the inherited Cargo environment, the two invocations disagree about
//! the fingerprint of every unit they share, and each run undoes the other's
//! work. Measured on this crate, alternating cost ~15s per test on every
//! `cargo nextest run -p assay-cli` after the first, with which test paid it
//! decided by scheduling order.

use std::process::Command;

/// The Cargo that built this test, rather than whichever one is on `PATH`.
///
/// Baked in at compile time, so a nested build uses the toolchain pinned by
/// `rust-toolchain.toml` instead of resolving through the rustup shim, which
/// can select a different one.
pub fn cargo_bin() -> &'static str {
    option_env!("CARGO").unwrap_or("cargo")
}

/// Remove the per-crate variables Cargo injects into *this* test process, so a
/// nested Cargo invocation sees the environment a plain shell would give it.
///
/// Build scripts track these variables, so leaving them in place marks units
/// dirty in both directions: flipping `CARGO_MANIFEST_DIR` between unset (a
/// shell `cargo build`) and `crates/assay-cli` (inherited here) rebuilds
/// `ring`'s build script and cascades through the rustls/reqwest stack into
/// `assay-evidence`, `assay-adapter-api`, `assay-core`, `assay-metrics` and
/// `assay-mcp-server` — 14 crates, ~15s, every alternation.
///
/// The list states an intent rather than a complete set. It removes what
/// identifies *this crate* to a child process; several arms never fire under
/// `cargo test` or `cargo nextest` today and are matched defensively.
/// `CARGO_BIN_EXE_*` is matched by prefix because nextest sets it at runtime
/// even though cargo does not.
///
/// Three deliberate exclusions, none of them oversights:
///   - configuration the *user* set (`CARGO_HOME`, `CARGO_TARGET_DIR`,
///     `CARGO_NET_OFFLINE`, `RUSTFLAGS`): a shell would pass these through too,
///     and dropping them would change where the build writes or whether it may
///     reach the network;
///   - `CARGO` itself, which the nested invocation overwrites anyway;
///   - the dynamic-library search path (`LD_LIBRARY_PATH`, and nextest's
///     `NEXTEST_DYLD_FALLBACK_LIBRARY_PATH`): no build-script fingerprint tracks
///     it, so it costs nothing, and removing it could break linking.
pub fn strip_cargo_crate_env(cmd: &mut Command) {
    for (key, _) in std::env::vars_os() {
        let Some(key) = key.to_str() else { continue };
        let injected = key.starts_with("CARGO_PKG_")
            || key.starts_with("CARGO_BIN_EXE_")
            || matches!(
                key,
                "CARGO_MANIFEST_DIR"
                    | "CARGO_MANIFEST_PATH"
                    | "CARGO_MANIFEST_LINKS"
                    | "CARGO_CRATE_NAME"
                    | "CARGO_BIN_NAME"
                    | "CARGO_PRIMARY_PACKAGE"
                    | "CARGO_TARGET_TMPDIR"
                    | "OUT_DIR"
            );
        if injected {
            cmd.env_remove(key);
        }
    }
}
