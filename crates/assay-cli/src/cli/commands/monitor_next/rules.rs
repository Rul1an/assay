//! What `assay monitor` does with one `runtime_monitor` rule.
//!
//! A rule is matched in userspace against one kind of event, installed in the kernel, both, or
//! refused when the policy loads. There is no rule the monitor loads and then ignores: that is how
//! a `proc_exec` rule once read as protection while matching nothing.

#[cfg(target_os = "linux")]
use super::{normalize, output};
#[cfg(target_os = "linux")]
use assay_common::MonitorEvent;
use assay_common::{EVENT_EXEC, EVENT_OPENAT};
use assay_core::mcp::runtime_features::{
    MonitorAction, MonitorRule, MonitorRuleType, RuntimeMonitorConfig,
};

#[derive(Debug)]
pub(crate) struct ActiveRule {
    pub(crate) id: String,
    pub(crate) action: MonitorAction,
    /// The event this rule is matched against: `EVENT_OPENAT` or `EVENT_EXEC`.
    pub(crate) event_type: u32,
    // NOTE: Despite the name, `allow` represents patterns that trigger this rule.
    pub(crate) allow: globset::GlobSet,
    pub(crate) deny: Option<globset::GlobSet>,
}

pub(crate) fn compile_globset(globs: &[String]) -> anyhow::Result<globset::GlobSet> {
    let mut b = globset::GlobSetBuilder::new();
    for g in globs {
        b.add(globset::Glob::new(g)?);
    }
    Ok(b.build()?)
}

/// The one reading of a rule. `Ok(Some(_))`: matched in userspace (a `file_open` rule with `deny`
/// or `trigger_kill` is also installed in the kernel by the enforcement compiler). `Ok(None)`:
/// kernel only. `Err(reason)`: this monitor cannot act on it.
///
/// `kill_switch_enabled` is whether the policy has an enabled `kill_switch`; see
/// [`kill_switch_enabled`].
fn plan_rule(rule: &MonitorRule, kill_switch_enabled: bool) -> Result<Option<ActiveRule>, String> {
    let matcher = &rule.match_config;
    let event_type = match rule.rule_type {
        MonitorRuleType::FileOpen => EVENT_OPENAT,
        MonitorRuleType::ProcExec => {
            if rule.action == MonitorAction::Deny {
                return Err(
                    "proc_exec cannot be denied: the monitor sees an exec once the new \
                            program has started. Use trigger_kill, alert or log"
                        .to_string(),
                );
            }
            // Nothing is installed in the kernel for an exec rule, so the kill switch is all
            // that carries out trigger_kill. Without it the rule would log and stop nothing.
            if rule.action == MonitorAction::TriggerKill && !kill_switch_enabled {
                return Err(
                    "trigger_kill on proc_exec needs an enabled kill_switch: without one the \
                     monitor would only log the exec. Enable kill_switch, or use alert or log"
                        .to_string(),
                );
            }
            EVENT_EXEC
        }
        MonitorRuleType::NetConnect => {
            if !matches!(
                rule.action,
                MonitorAction::Deny | MonitorAction::TriggerKill
            ) {
                return Err(
                    "net_connect rules are installed in the kernel as deny rules; \
                            action log or alert has no effect"
                        .to_string(),
                );
            }
            if matcher.dest_globs.is_empty() {
                return Err("has no dest_globs, so it can never match".to_string());
            }
            return Ok(None);
        }
    };
    if matcher.path_globs.is_empty() {
        return Err("has no path_globs, so it can never match".to_string());
    }
    let allow = compile_globset(&matcher.path_globs)
        .map_err(|error| format!("path_globs do not compile: {error}"))?;
    let deny = matcher
        .not
        .as_ref()
        .map(|not| compile_globset(&not.path_globs))
        .transpose()
        .map_err(|error| format!("not.path_globs do not compile: {error}"))?;
    Ok(Some(ActiveRule {
        id: rule.id.clone(),
        action: rule.action.clone(),
        event_type,
        allow,
        deny,
    }))
}

/// The rules matched in userspace. Call [`unenforceable_rules`] first: a rule refused there is
/// absent here.
pub(crate) fn compile_active_rules(
    runtime_config: Option<&RuntimeMonitorConfig>,
    kill_switch_enabled: bool,
) -> Vec<ActiveRule> {
    runtime_config
        .map(|cfg| {
            cfg.rules
                .iter()
                .filter_map(|rule| plan_rule(rule, kill_switch_enabled).ok().flatten())
                .collect()
        })
        .unwrap_or_default()
}

/// One line per rule this monitor cannot act on, naming the rule and the reason. The caller
/// refuses to start on a non-empty answer.
pub(crate) fn unenforceable_rules(
    cfg: &RuntimeMonitorConfig,
    kill_switch_enabled: bool,
) -> Vec<String> {
    cfg.rules
        .iter()
        .filter_map(|rule| {
            plan_rule(rule, kill_switch_enabled)
                .err()
                .map(|reason| format!("rule '{}': {reason}", rule.id))
        })
        .collect()
}

/// Whether the policy's kill switch will carry out a kill: the section is present and enabled.
/// The event handler applies the same reading when a `trigger_kill` rule matches.
pub(crate) fn kill_switch_enabled(
    kill_switch: Option<&assay_core::mcp::runtime_features::KillSwitchConfig>,
) -> bool {
    kill_switch.is_some_and(|config| config.enabled)
}

/// The first rule that matches an event of `event_type` on `path`, in policy order.
pub(crate) fn matching_rule<'a>(
    event_type: u32,
    path: &str,
    rules: &'a [ActiveRule],
) -> Option<&'a ActiveRule> {
    rules.iter().find(|rule| {
        rule.event_type == event_type
            && rule.allow.is_match(path)
            && !rule.deny.as_ref().is_some_and(|deny| deny.is_match(path))
    })
}

#[cfg(target_os = "linux")]
pub(crate) fn find_violation_rule<'a>(
    event: &MonitorEvent,
    rules: &'a [ActiveRule],
) -> Option<&'a ActiveRule> {
    // Both events carry a path as a C string: the file opened, or the image that was exec'd, as
    // the kernel recorded it.
    if rules.is_empty() || !matches!(event.event_type, EVENT_OPENAT | EVENT_EXEC) {
        return None;
    }

    let raw = output::decode_utf8_cstr(&event.data);
    let path = normalize::normalize_path_syntactic(&raw);
    matching_rule(event.event_type, &path, rules)
}

#[cfg(test)]
mod tests {
    use super::*;
    use assay_common::{EVENT_CONNECT, EVENT_EXEC, EVENT_OPENAT};
    use assay_core::mcp::runtime_features::RuntimeMonitorConfig;

    fn config(rules_yaml: &str) -> RuntimeMonitorConfig {
        serde_yaml::from_str(&format!("enabled: true\nrules:\n{rules_yaml}"))
            .expect("monitor config")
    }

    const EXEC_KILL: &str = "  - id: no-curl\n    type: proc_exec\n    match:\n      path_globs: ['**/curl']\n      not:\n        path_globs: ['/opt/safe/**']\n    action: trigger_kill\n";
    const KILL_SWITCH_ON: bool = true;
    const KILL_SWITCH_OFF: bool = false;
    const FILE_DENY: &str = "  - id: no-secrets\n    type: file_open\n    match:\n      path_globs: ['/etc/secret/**']\n    action: deny\n";

    /// The defect: a `proc_exec` rule loaded and then matched nothing, whatever ran.
    #[test]
    fn a_proc_exec_rule_matches_the_exec_it_names_and_only_exec_events() {
        let cfg = config(&format!("{EXEC_KILL}{FILE_DENY}"));
        assert_eq!(
            unenforceable_rules(&cfg, KILL_SWITCH_ON),
            Vec::<String>::new()
        );
        let rules = compile_active_rules(Some(&cfg), KILL_SWITCH_ON);

        let hit = matching_rule(EVENT_EXEC, "/usr/bin/curl", &rules).expect("exec of curl matches");
        assert_eq!(hit.id, "no-curl");
        assert!(matching_rule(EVENT_EXEC, "/usr/bin/true", &rules).is_none());
        // A relative exec is recorded without its directory; `**/curl` still names it.
        assert!(matching_rule(EVENT_EXEC, "curl", &rules).is_some());
        // The rule's own exception still holds.
        assert!(matching_rule(EVENT_EXEC, "/opt/safe/curl", &rules).is_none());
        // An exec rule does not fire on a file open of the same path, nor a file rule on an exec.
        assert!(matching_rule(EVENT_OPENAT, "/usr/bin/curl", &rules).is_none());
        assert!(matching_rule(EVENT_EXEC, "/etc/secret/key", &rules).is_none());
        assert_eq!(
            matching_rule(EVENT_OPENAT, "/etc/secret/key", &rules).map(|r| r.id.as_str()),
            Some("no-secrets")
        );
        // Events no rule type reads never match.
        assert!(matching_rule(EVENT_CONNECT, "/usr/bin/curl", &rules).is_none());
    }

    /// Every rule is either acted on or refused at load; none is loaded and ignored.
    #[test]
    fn a_rule_the_monitor_cannot_act_on_is_named_at_load() {
        let cases = [
            (
                "  - id: exec-deny\n    type: proc_exec\n    match:\n      path_globs: ['**/curl']\n    action: deny\n",
                "rule 'exec-deny': proc_exec cannot be denied",
            ),
            (
                "  - id: net-log\n    type: net_connect\n    match:\n      dest_globs: ['10.0.0.0/8']\n    action: log\n",
                "rule 'net-log': net_connect",
            ),
            (
                "  - id: exec-empty\n    type: proc_exec\n    match: {}\n    action: alert\n",
                "rule 'exec-empty': has no path_globs",
            ),
            (
                "  - id: file-empty\n    type: file_open\n    match:\n      dest_globs: ['10.0.0.0/8']\n    action: deny\n",
                "rule 'file-empty': has no path_globs",
            ),
            (
                "  - id: net-empty\n    type: net_connect\n    match:\n      path_globs: ['/x']\n    action: deny\n",
                "rule 'net-empty': has no dest_globs",
            ),
            (
                "  - id: bad-glob\n    type: file_open\n    match:\n      path_globs: ['/etc/[']\n    action: log\n",
                "rule 'bad-glob': path_globs do not compile",
            ),
            // An exception that does not compile used to be dropped, which widens the rule: every
            // path it was meant to spare then matches.
            (
                "  - id: bad-not\n    type: file_open\n    match:\n      path_globs: ['/etc/**']\n      not:\n        path_globs: ['/etc/[']\n    action: trigger_kill\n",
                "rule 'bad-not': not.path_globs do not compile",
            ),
        ];
        for (yaml, want) in cases {
            let refused = unenforceable_rules(&config(yaml), KILL_SWITCH_ON);
            assert_eq!(refused.len(), 1, "{yaml} -> {refused:?}");
            assert!(
                refused[0].starts_with(want),
                "{:?} does not start with {want:?}",
                refused[0]
            );
            // A refused rule is not also matched in userspace.
            assert!(compile_active_rules(Some(&config(yaml)), KILL_SWITCH_ON).is_empty());
        }
        // Controls: the rules the monitor does act on are not refused.
        for yaml in [
            EXEC_KILL,
            FILE_DENY,
            "  - id: exec-log\n    type: proc_exec\n    match:\n      path_globs: ['**/sh']\n    action: log\n",
            "  - id: net-deny\n    type: net_connect\n    match:\n      dest_globs: ['10.0.0.0/8']\n    action: deny\n",
        ] {
            assert_eq!(unenforceable_rules(&config(yaml), KILL_SWITCH_ON), Vec::<String>::new(), "{yaml}");
        }
    }

    /// `trigger_kill` on an exec is carried out by the kill switch alone: nothing is installed in
    /// the kernel for it. Without an enabled kill switch the rule would log and stop nothing.
    #[test]
    fn a_proc_exec_kill_rule_needs_the_kill_switch() {
        let refused = unenforceable_rules(&config(EXEC_KILL), KILL_SWITCH_OFF);
        assert_eq!(refused.len(), 1, "{refused:?}");
        assert!(
            refused[0].starts_with(
                "rule 'no-curl': trigger_kill on proc_exec needs an enabled kill_switch"
            ),
            "{:?}",
            refused[0]
        );
        assert!(compile_active_rules(Some(&config(EXEC_KILL)), KILL_SWITCH_OFF).is_empty());

        // A file rule with trigger_kill is still blocked in the kernel without the kill switch, so
        // it is acted on; so is an exec rule that only reports.
        let file_kill = "  - id: f\n    type: file_open\n    match:\n      path_globs: ['/etc/secret/**']\n    action: trigger_kill\n";
        let exec_alert = "  - id: e\n    type: proc_exec\n    match:\n      path_globs: ['**/sh']\n    action: alert\n";
        for yaml in [file_kill, exec_alert] {
            assert_eq!(
                unenforceable_rules(&config(yaml), KILL_SWITCH_OFF),
                Vec::<String>::new(),
                "{yaml}"
            );
        }
    }

    /// The first rule in the policy that matches is the one reported and acted on.
    #[test]
    fn the_first_matching_rule_in_policy_order_wins() {
        let two = "  - id: first\n    type: proc_exec\n    match:\n      path_globs: ['**/curl']\n    action: log\n  - id: second\n    type: proc_exec\n    match:\n      path_globs: ['/usr/**']\n    action: trigger_kill\n";
        let rules = compile_active_rules(Some(&config(two)), KILL_SWITCH_ON);
        assert_eq!(
            matching_rule(EVENT_EXEC, "/usr/bin/curl", &rules).map(|r| r.id.as_str()),
            Some("first")
        );
    }
}
