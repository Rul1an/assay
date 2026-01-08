pub mod fmt;
pub mod migrate;
pub mod validate;

use crate::cli::args::{PolicyArgs, PolicyCommand};

pub async fn run(args: PolicyArgs) -> anyhow::Result<i32> {
    match args.cmd {
        PolicyCommand::Validate(a) => validate::run(a).await,
        PolicyCommand::Migrate(a) => migrate::run(a).await,
        PolicyCommand::Fmt(a) => fmt::run(a).await,
    }
}
