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

/// The connect endpoint an event observed, if it is one.
///
/// Both allowed and blocked connects count as observations: the question a refutation asks is
/// whether the kernel saw the workload try to reach that peer at all, and a blocked attempt is a
/// sighting, not an absence. Whether the connection then succeeded is a different question this
/// surface cannot answer, which is why `EgressRefutation::Refuted` names the surface it watched
/// rather than the world.
pub(crate) fn observed_peer(event: &assay_common::MonitorEvent) -> Option<String> {
    match event.event_type {
        // Both carry the PROJECTED payload the cgroup hook writes, read from the kernel's own
        // `bpf_sock_addr`, so one decoder serves both. EVENT_CONNECT is deliberately NOT here: the
        // tracepoint emits a raw sockaddr copied out of userspace before the kernel takes it, which
        // a process can change underneath, and a peer set that grounds a refutation cannot rest on
        // an input the subject controls.
        assay_common::EVENT_CONNECT_OBSERVED | assay_common::EVENT_CONNECT_BLOCKED => {
            assay_monitor::events::decode_blocked_socket_payload(&event.data).map(|s| s.endpoint())
        }
        _ => None,
    }
}
