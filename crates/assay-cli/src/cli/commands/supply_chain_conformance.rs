//! `assay registry supply-chain-conformance` (A5a-1): emit the `assay.supply_chain_conformance.v0`
//! carrier by running the existing `assay_registry::supply_chain::verify_supply_chain` producer over a
//! local, caller-supplied input descriptor.
//!
//! This is a thin CLI boundary around an existing producer. It introduces NO verifier/trust/policy
//! semantics. It performs OFFLINE checks over the supplied inputs and reports carrier status; it does
//! not assert supply-chain safety, policy approval, compliance, Sigstore trust, Rekor inclusion, issuer
//! identity, or artifact runtime integrity.
//!
//! Scope: `none`, `unsupported`, and `dsse` (pinned-key, offline) provenance. The `dsse` path verifies a
//! local DSSE-wrapped in-toto/SLSA statement against a caller-supplied pinned Ed25519 key via the existing
//! `assay_registry` verifier - NO cryptography is implemented here, only descriptor->VerifyInput wiring and
//! safe descriptor-relative file resolution. The keyless `sigstore_bundle` path is modeled in the descriptor
//! but explicitly DEFERRED: it is rejected with a clear non-zero, never silently ignored.

use std::io::Write;
use std::path::Path;

use crate::cli::args::SupplyChainConformanceArgs;
use crate::exit_codes::{EXIT_CONFIG_ERROR, EXIT_INFRA_ERROR, EXIT_SUCCESS};

mod descriptor;

#[cfg(test)]
mod tests;

use descriptor::{build_carrier, EmitErr};

pub async fn run(args: SupplyChainConformanceArgs) -> anyhow::Result<i32> {
    // `--offline` is a guard, not a mode switch: the producer performs no network I/O by construction.
    // There is no fetch-capable path in this slice, so the guard is trivially satisfied; if one is ever
    // introduced it MUST hard-fail here before any fetch.
    let _ = args.offline;

    let raw = match std::fs::read_to_string(&args.input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "[config_error] cannot read input descriptor {}: {e}",
                args.input
            );
            return Ok(EXIT_CONFIG_ERROR);
        }
    };

    // dsse `envelope_path`/`trusted_key_path` resolve relative to the descriptor file's directory.
    let base_dir = Path::new(&args.input)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let carrier = match build_carrier(&raw, base_dir) {
        Ok(c) => c,
        Err(EmitErr { code, msg }) => {
            eprintln!("{msg}");
            return Ok(code);
        }
    };

    let rendered = format!("{}\n", serde_json::to_string_pretty(&carrier)?);
    // Output-write failures are an infra/output problem regardless of the target: stdout and file
    // writes route through the same mapping, so a broken pipe on stdout is EXIT_INFRA_ERROR just like
    // an unwritable file path (never the generic `?` bubble).
    let write_result = if args.out == "-" {
        std::io::stdout().write_all(rendered.as_bytes())
    } else {
        std::fs::write(&args.out, &rendered)
    };
    let target = if args.out == "-" {
        "stdout"
    } else {
        args.out.as_str()
    };
    Ok(map_write_result(target, write_result))
}

/// Map an output-write result to an exit code. A write failure is an infra/output problem
/// (`EXIT_INFRA_ERROR`), applied uniformly to stdout and file targets so the exit-code contract is
/// the same whatever `--out` points at.
fn map_write_result(target: &str, result: std::io::Result<()>) -> i32 {
    match result {
        Ok(()) => EXIT_SUCCESS,
        Err(e) => {
            eprintln!("[infra_error] cannot write output ({target}): {e}");
            EXIT_INFRA_ERROR
        }
    }
}
