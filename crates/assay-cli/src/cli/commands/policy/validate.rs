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
    let resolved = super::resolved::load_resolved(&bytes)
        .map_err(|error| super::classify_load_error(&args.input, error))?;

    eprintln!("✔ Policy OK: {}", args.input.display());
    for note in assay_core::mcp::policy::keys_wrap_does_not_act_on(&bytes) {
        eprintln!("[assay] WARNING: {note}");
    }
    for refusal in monitor_refusals(&resolved.policy) {
        eprintln!("[assay] WARNING: assay monitor will refuse this policy: {refusal}");
    }
    if args.is_json() {
        let mut summary = summary_from_outcome(&RunOutcome::success(), true);
        summary.message = None;
        return Ok(write_summary_stdout(&summary));
    }
    Ok(exit_codes::OK)
}

/// The `runtime_monitor` rules `assay monitor` will refuse at start, by the monitor's own reading.
///
/// The policy is valid for every other reader, so this is a warning and not a failure. It is the
/// monitor's function that answers, with the kill switch read as the monitor reads it, so the two
/// cannot disagree. A disabled `runtime_monitor` is not read at all and yields nothing.
fn monitor_refusals(policy: &assay_core::mcp::policy::McpPolicy) -> Vec<String> {
    use crate::cli::commands::monitor::monitor_next::rules;
    match &policy.runtime_monitor {
        Some(config) if config.enabled => rules::unenforceable_rules(
            config,
            rules::kill_switch_enabled(policy.kill_switch.as_ref()),
        ),
        _ => Vec::new(),
    }
}
