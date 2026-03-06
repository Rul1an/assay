#[derive(clap::Args, Clone, Debug)]
pub struct EvidenceArgs {
    #[command(subcommand)]
    pub cmd: crate::cli::commands::evidence::EvidenceCmd,
}
