use std::path::Path;

use crate::cli::args::PolicyValidateArgs;
use crate::cli_failure::{emit_summary_stdout, summary_from_outcome, CliFailure};
use crate::exit_codes;
use crate::exit_codes::RunOutcome;
use anyhow::Result;

pub async fn run(args: PolicyValidateArgs) -> Result<i32> {
    if args.deny_deprecations {
        std::env::set_var("ASSAY_STRICT_DEPRECATIONS", "1");
    }

    // Let core handle parsing + auto-migration warnings.
    let policy = assay_core::mcp::policy::McpPolicy::from_file(&args.input)
        .map_err(|error| classify_load_error(&args.input, error))?;

    // Force schema compilation so failures happen here (not at runtime).
    policy
        .try_compile_all_schemas()
        .map_err(|error| anyhow::anyhow!("policy schemas failed to compile: {error}"))?;

    eprintln!("✔ Policy OK: {}", args.input.display());
    if args.is_json() {
        let mut summary = summary_from_outcome(&RunOutcome::success(), true);
        summary.message = None;
        emit_summary_stdout(&summary)?;
    }
    Ok(exit_codes::OK)
}

fn classify_load_error(path: &Path, error: anyhow::Error) -> anyhow::Error {
    if error
        .downcast_ref::<assay_core::mcp::policy::McpPolicyError>()
        .is_some_and(|error| error.is_parse_failure())
    {
        return CliFailure::policy_parse(path, error).into();
    }
    error.context(format!("failed to load policy {}", path.display()))
}
