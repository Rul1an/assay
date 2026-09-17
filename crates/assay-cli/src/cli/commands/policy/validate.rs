use crate::cli::args::PolicyValidateArgs;
use crate::cli_failure::{summary_from_outcome, write_summary_stdout};
use crate::exit_codes;
use crate::exit_codes::RunOutcome;
use anyhow::Result;

pub async fn run(args: PolicyValidateArgs) -> Result<i32> {
    if args.deny_deprecations {
        std::env::set_var("ASSAY_STRICT_DEPRECATIONS", "1");
    }

    let bytes = super::resolved::read_bounded(&args.input).map_err(|error| {
        super::classify_load_error(
            &args.input,
            error
                .context(format!("failed to read policy {}", args.input.display()))
                .context(format!("failed to read policy {}", args.input.display())),
        )
    })?;
    let _resolved = super::resolved::load_resolved(&bytes)
        .map_err(|error| super::classify_load_error(&args.input, error))?;

    eprintln!("✔ Policy OK: {}", args.input.display());
    if args.is_json() {
        let mut summary = summary_from_outcome(&RunOutcome::success(), true);
        summary.message = None;
        return Ok(write_summary_stdout(&summary));
    }
    Ok(exit_codes::OK)
}
