#[cfg(target_os = "linux")]
use super::{normalize, output};
#[cfg(target_os = "linux")]
use assay_common::MonitorEvent;
use assay_common::EVENT_OPENAT;

#[derive(Debug)]
pub(crate) struct ActiveRule {
    pub(crate) id: String,
    pub(crate) action: assay_core::mcp::runtime_features::MonitorAction,
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

pub(crate) fn compile_active_rules(
    runtime_config: Option<&assay_core::mcp::runtime_features::RuntimeMonitorConfig>,
) -> Vec<ActiveRule> {
    let mut rules = Vec::new();
    if let Some(cfg) = runtime_config {
        for r in &cfg.rules {
            let kind = r.rule_type.clone();
            let mc = &r.match_config;

            if !matches!(
                kind,
                assay_core::mcp::runtime_features::MonitorRuleType::FileOpen
            ) {
                continue;
            }

            match compile_globset(&mc.path_globs) {
                Ok(allow) => {
                    let deny = mc
                        .not
                        .as_ref()
                        .map(|n| compile_globset(&n.path_globs))
                        .transpose()
                        .unwrap_or(None);
                    rules.push(ActiveRule {
                        id: r.id.clone(),
                        action: r.action.clone(),
                        allow,
                        deny,
                    });
                }
                Err(e) => {
                    eprintln!("Warning: Failed to compile glob for rule {}: {}", r.id, e);
                }
            }
        }
    }
    rules
}

/// Rules this monitor cannot act on. Nothing is refused yet: this is the behaviour before the
/// change, extracted so it can be tested.
pub(crate) fn unenforceable_rules(
    _cfg: &assay_core::mcp::runtime_features::RuntimeMonitorConfig,
) -> Vec<String> {
    Vec::new()
}

/// The first rule that matches an event of `event_type` on `path`.
pub(crate) fn matching_rule<'a>(
    event_type: u32,
    path: &str,
    rules: &'a [ActiveRule],
) -> Option<&'a ActiveRule> {
    if event_type != EVENT_OPENAT {
        return None;
    }
    for r in rules {
        if r.allow.is_match(path) {
            let blocked = r.deny.as_ref().map(|d| d.is_match(path)).unwrap_or(false);
            if !blocked {
                return Some(r);
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
pub(crate) fn find_violation_rule<'a>(
    event: &MonitorEvent,
    rules: &'a [ActiveRule],
) -> Option<&'a ActiveRule> {
    if rules.is_empty() || event.event_type != EVENT_OPENAT {
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
