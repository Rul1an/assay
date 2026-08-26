pub mod fmt;
pub mod migrate;
pub mod resolve;
pub mod validate;

use std::path::Path;

use crate::cli::args::{PolicyArgs, PolicyCommand};
use crate::cli_failure::CliFailure;

/// Shared load-error classifier for `policy validate` and `policy resolve`.
pub(crate) fn classify_load_error(path: &Path, error: anyhow::Error) -> anyhow::Error {
    if error
        .downcast_ref::<assay_core::mcp::policy::McpPolicyError>()
        .is_some_and(|error| error.is_parse_failure())
    {
        return CliFailure::policy_parse(path, error).into();
    }
    error.context(format!("failed to load policy {}", path.display()))
}

pub async fn run(args: PolicyArgs) -> anyhow::Result<i32> {
    match args.cmd {
        PolicyCommand::Generate(a) => super::generate::run(a),
        PolicyCommand::Record(a) => super::record::run(a).await,
        PolicyCommand::Validate(a) => validate::run(a).await,
        PolicyCommand::Migrate(a) => migrate::run(a).await,
        PolicyCommand::Fmt(a) => fmt::run(a).await,
        PolicyCommand::Resolve(a) => resolve::run(a).await,
    }
}
