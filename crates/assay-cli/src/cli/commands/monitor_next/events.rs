#[cfg(target_os = "linux")]
use super::{output, rules, syscall_linux};
#[cfg(target_os = "linux")]
use crate::cli::commands::monitor::MonitorArgs;

#[cfg(target_os = "linux")]
async fn maybe_enforce_violation(
    event: &assay_common::MonitorEvent,
    rule: &rules::ActiveRule,
    kill_config: Option<&assay_core::mcp::runtime_features::KillSwitchConfig>,
    quiet: bool,
) {
    output::log_violation(event.pid, &rule.id, quiet);

    if rule.action != assay_core::mcp::runtime_features::MonitorAction::TriggerKill {
        return;
    }

    let default_mode = assay_core::mcp::runtime_features::KillMode::Graceful;
    let default_grace = 3000;

    let (enabled, mode, grace) = if let Some(kc) = kill_config {
        let trigger = kc.triggers.iter().find(|t| t.on_rule == rule.id);
        let mode = trigger
            .and_then(|t| t.mode.clone())
            .unwrap_or(kc.mode.clone());
        (kc.enabled, mode, kc.grace_period_ms)
    } else {
        (false, default_mode, default_grace)
    };

    if enabled {
        output::log_kill(event.pid, &mode, grace, quiet);
        syscall_linux::kill_pid(event.pid, mode, grace).await;
    }
}

#[cfg(target_os = "linux")]
pub(crate) async fn handle_event(
    event: &assay_common::MonitorEvent,
    args: &MonitorArgs,
    ruleset: &[rules::ActiveRule],
    kill_config: Option<&assay_core::mcp::runtime_features::KillSwitchConfig>,
) {
    if let Some(rule) = rules::find_violation_rule(event, ruleset) {
        maybe_enforce_violation(event, rule, kill_config, args.quiet).await;
    }

    output::log_monitor_event(event, args);
}
