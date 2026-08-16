#![allow(unsafe_code)]
use clap::Parser;

pub mod aee_run_context;
pub mod aee_seal;
pub mod aee_seal_envelope;
pub mod aee_seal_key;
#[cfg(test)]
mod aee_seal_round_trip;
pub mod aee_trust_set;
pub mod backend;
pub mod caps;
mod cli;
mod cli_failure;
pub mod diagnostics;
pub mod enforcement_health_v1;
mod env_filter;
mod evidence_verify_reason;
pub mod exit_codes;
pub mod fs;
pub mod landlock_check;
pub mod landlock_net;
pub mod metrics;
pub mod packs;
pub mod policy;
pub mod profile;
pub mod setup;
mod templates;

use cli::args::Cli;
use cli::commands::dispatch;
use cli_failure::CliFailure;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    env_logger::init();
    let cli = Cli::parse();
    let machine_output_verify_enabled = cli.machine_output_verify_enabled();
    let legacy_mode = std::env::var("MCP_CONFIG_LEGACY").ok().as_deref() == Some("1");
    let code = match dispatch(cli, legacy_mode).await {
        Ok(code) => code,
        Err(error) => match error.downcast::<CliFailure>() {
            Ok(failure) => failure.emit(machine_output_verify_enabled),
            Err(error) => {
                eprintln!("fatal: {error:?}");
                2 // CONFIG_ERROR from cli::commands::exit_codes::CONFIG_ERROR ideally, but hardcoded 2 is safe here
            }
        },
    };
    std::process::exit(code);
}
