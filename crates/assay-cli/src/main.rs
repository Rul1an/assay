use clap::Parser;

mod cli;
mod templates;

use cli::args::Cli;
use cli::commands::dispatch;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let cli = Cli::parse();
    let code = match dispatch(cli).await {
        Ok(code) => code,
        Err(e) => {
            eprintln!("fatal: {e:?}");
            2 // CONFIG_ERROR from cli::commands::exit_codes::CONFIG_ERROR ideally, but hardcoded 2 is safe here
        }
    };
    std::process::exit(code);
}
