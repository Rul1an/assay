use clap::Parser;

mod cli;
pub mod packs;
mod templates;
pub mod cgroup;

use cli::args::Cli;
use cli::commands::dispatch;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    env_logger::init();
    let cli = Cli::parse();
    let legacy_mode = std::env::var("MCP_CONFIG_LEGACY").ok().as_deref() == Some("1");
    let code = match dispatch(cli, legacy_mode).await {
        Ok(code) => code,
        Err(e) => {
            eprintln!("fatal: {e:?}");
            2 // CONFIG_ERROR from cli::commands::exit_codes::CONFIG_ERROR ideally, but hardcoded 2 is safe here
        }
    };
    std::process::exit(code);
}
