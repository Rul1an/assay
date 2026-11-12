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
fn plan_rule(rule: &MonitorRule) -> Result<Option<ActiveRule>, String> {
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
) -> Vec<ActiveRule> {
    runtime_config
        .map(|cfg| {
            cfg.rules
                .iter()
                .filter_map(|rule| plan_rule(rule).ok().flatten())
                .collect()
        })
        .unwrap_or_default()
}

/// One line per rule this monitor cannot act on, naming the rule and the reason. The caller
/// refuses to start on a non-empty answer.
pub(crate) fn unenforceable_rules(cfg: &RuntimeMonitorConfig) -> Vec<String> {
    cfg.rules
        .iter()
        .filter_map(|rule| {
            plan_rule(rule)
                .err()
                .map(|reason| format!("rule '{}': {reason}", rule.id))
        })
        .collect()
}

/// The first rule that matches an event of `event_type` on `path`.
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
    const FILE_DENY: &str = "  - id: no-secrets\n    type: file_open\n    match:\n      path_globs: ['/etc/secret/**']\n    action: deny\n";

    /// The defect: a `proc_exec` rule loaded and then matched nothing, whatever ran.
    #[test]
    fn a_proc_exec_rule_matches_the_exec_it_names_and_only_exec_events() {
        let cfg = config(&format!("{EXEC_KILL}{FILE_DENY}"));
        assert_eq!(unenforceable_rules(&cfg), Vec::<String>::new());
        let rules = compile_active_rules(Some(&cfg));

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
        ];
        for (yaml, want) in cases {
            let refused = unenforceable_rules(&config(yaml));
            assert_eq!(refused.len(), 1, "{yaml} -> {refused:?}");
            assert!(
                refused[0].starts_with(want),
                "{:?} does not start with {want:?}",
                refused[0]
            );
            // A refused rule is not also matched in userspace.
            assert!(compile_active_rules(Some(&config(yaml))).is_empty());
        }
        // Controls: the rules the monitor does act on are not refused.
        for yaml in [
            EXEC_KILL,
            FILE_DENY,
            "  - id: exec-log\n    type: proc_exec\n    match:\n      path_globs: ['**/sh']\n    action: log\n",
            "  - id: net-deny\n    type: net_connect\n    match:\n      dest_globs: ['10.0.0.0/8']\n    action: deny\n",
        ] {
            assert_eq!(unenforceable_rules(&config(yaml)), Vec::<String>::new(), "{yaml}");
        }
    }
}
