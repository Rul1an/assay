//! What `assay monitor` does with a policy: each `runtime_monitor` rule, and the kill switch.
//!
//! A rule is matched in userspace against one kind of event, installed in the kernel, both, or
//! refused when the policy loads. Nothing the monitor loads is then ignored: not a rule, which is
//! how a `proc_exec` rule once read as protection while matching nothing, and not a key or a value,
//! which is how `kill_children: true` once read as a kill switch that stops a process tree while
//! one process was signalled.

use super::view::{KillSwitchView, RuleView, RuntimeMonitorView};
#[cfg(target_os = "linux")]
use super::{normalize, output};
#[cfg(target_os = "linux")]
use assay_common::MonitorEvent;
use assay_common::{EVENT_EXEC, EVENT_OPENAT};
use assay_core::mcp::runtime_features::{KillMode, MonitorAction, MonitorRuleType};

/// The monitor's reading of a policy file: [`read_monitor_policy`].
#[derive(Debug)]
pub(crate) struct MonitorPolicy {
    /// `runtime_monitor.enabled` is false. The monitor does nothing, and reads nothing else.
    pub(crate) disabled: bool,
    /// The `runtime_monitor` rules, or `None` when the policy has no `runtime_monitor` section.
    pub(crate) rules: Option<Vec<RuleView>>,
    pub(crate) kill: KillPlan,
    /// One line per key, value or rule the monitor cannot act on. The monitor refuses to start on
    /// any, and `assay policy validate` prints them.
    pub(crate) refusals: Vec<String>,
}

/// The one reading of a policy's `runtime_monitor` and `kill_switch` sections, for `assay monitor`
/// and for `assay policy validate`, so the two cannot disagree.
///
/// Call it after the policy has loaded (`McpPolicy::from_slice`): a section of the wrong shape is
/// reported there, with the loader's error kind.
pub(crate) fn read_monitor_policy(policy_file: &[u8]) -> anyhow::Result<MonitorPolicy> {
    use assay_core::mcp::policy::read_section;

    let runtime = read_section::<RuntimeMonitorView>(policy_file, "runtime_monitor")?;
    if runtime
        .value
        .as_ref()
        .is_some_and(|section| !section.enabled)
    {
        return Ok(MonitorPolicy {
            disabled: true,
            rules: None,
            kill: KillPlan::none(),
            refusals: Vec::new(),
        });
    }
    let kill_switch = read_section::<KillSwitchView>(policy_file, "kill_switch")?;

    let mut refusals: Vec<String> = runtime
        .unread
        .iter()
        .chain(&kill_switch.unread)
        .map(|path| format!("{path:?}: assay monitor does not act on this key; remove it"))
        .collect();
    let rules = runtime.value.map(|section| section.rules);
    let kill = KillPlan::from_view(kill_switch.value.as_ref());
    if let Some(rules) = &rules {
        refusals.extend(unenforceable_rules(rules, kill.enabled));
    }
    if let Some(view) = &kill_switch.value {
        refusals.extend(trigger_refusals(view, rules.as_deref().unwrap_or_default()));
    }
    Ok(MonitorPolicy {
        disabled: false,
        rules,
        kill,
        refusals,
    })
}

/// What the kill switch does when a `trigger_kill` rule matches.
#[derive(Debug, Clone)]
pub(crate) struct KillPlan {
    /// The section is present and not switched off. Without it nothing is killed.
    pub(crate) enabled: bool,
    pub(crate) grace_period_ms: u64,
    mode: KillMode,
    /// `kill_switch.triggers`, in policy order: the mode for one rule, if the trigger sets one.
    triggers: Vec<(String, Option<KillMode>)>,
}

impl KillPlan {
    /// No `kill_switch` section: nothing is killed.
    pub(crate) fn none() -> Self {
        Self::from_view(None)
    }

    fn from_view(view: Option<&KillSwitchView>) -> Self {
        match view {
            Some(view) => Self {
                enabled: view.enabled,
                grace_period_ms: view.grace_period_ms,
                mode: view.mode.clone(),
                triggers: view
                    .triggers
                    .iter()
                    .map(|trigger| (trigger.on_rule.clone(), trigger.mode.clone()))
                    .collect(),
            },
            None => Self {
                enabled: false,
                grace_period_ms: 3000,
                mode: KillMode::Graceful,
                triggers: Vec::new(),
            },
        }
    }

    /// The mode a kill for `rule_id` uses: its trigger's, or the section's.
    pub(crate) fn mode_for(&self, rule_id: &str) -> KillMode {
        self.triggers
            .iter()
            .find(|(on_rule, _)| on_rule == rule_id)
            .and_then(|(_, mode)| mode.clone())
            .unwrap_or_else(|| self.mode.clone())
    }
}

/// A trigger only sets the kill mode for one rule; a `trigger_kill` rule is killed without one.
/// So a trigger that sets no mode, names a rule that never kills, or repeats a rule has no effect.
fn trigger_refusals(kill_switch: &KillSwitchView, rules: &[RuleView]) -> Vec<String> {
    let mut seen = Vec::new();
    let mut refusals = Vec::new();
    for trigger in &kill_switch.triggers {
        let rule = &trigger.on_rule;
        let reason = match rules.iter().find(|candidate| &candidate.id == rule) {
            None => Some(format!("rule '{rule}' is not a runtime_monitor rule")),
            Some(found) if found.action != MonitorAction::TriggerKill => {
                Some(format!("rule '{rule}' does not use trigger_kill"))
            }
            Some(_) if seen.contains(&rule) => {
                Some(format!("rule '{rule}' already has a trigger, and the first one is used"))
            }
            Some(_) if trigger.mode.is_none() => Some(format!(
                "the trigger for rule '{rule}' sets no mode, and kill_switch.mode applies without it"
            )),
            Some(_) => None,
        };
        seen.push(rule);
        if let Some(reason) = reason {
            refusals.push(format!(
                "kill_switch.triggers: {reason}, so this trigger has no effect"
            ));
        }
    }
    refusals
}

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

fn kind_name(kind: &MonitorRuleType) -> &'static str {
    match kind {
        MonitorRuleType::FileOpen => "file_open",
        MonitorRuleType::ProcExec => "proc_exec",
        MonitorRuleType::NetConnect => "net_connect",
    }
}

/// The one reading of a rule. `Ok(Some(_))`: matched in userspace (a `file_open` rule with `deny`
/// or `trigger_kill` is also installed in the kernel by the enforcement compiler). `Ok(None)`:
/// kernel only. `Err(reason)`: this monitor cannot act on it, or on a key it sets.
///
/// `kill_switch_enabled` is [`KillPlan::enabled`].
fn plan_rule(rule: &RuleView, kill_switch_enabled: bool) -> Result<Option<ActiveRule>, String> {
    let matcher = &rule.matcher;
    let kind = kind_name(&rule.kind);
    let event_type = match rule.kind {
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
            if !matcher.path_globs.is_empty() {
                return Err(format!("path_globs has no effect on a {kind} rule"));
            }
            if matcher.not.is_some() {
                return Err(format!("not has no effect on a {kind} rule"));
            }
            return Ok(None);
        }
    };
    if matcher.path_globs.is_empty() {
        return Err("has no path_globs, so it can never match".to_string());
    }
    if !matcher.dest_globs.is_empty() {
        return Err(format!("dest_globs has no effect on a {kind} rule"));
    }
    if let Some(not) = &matcher.not {
        if !not.dest_globs.is_empty() {
            return Err(format!("not.dest_globs has no effect on a {kind} rule"));
        }
        if not.not.is_some() {
            return Err("not.not has no effect: an exception has no exception".to_string());
        }
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

/// The rules matched in userspace. A rule [`unenforceable_rules`] refuses is absent here.
pub(crate) fn compile_active_rules(
    rules: Option<&[RuleView]>,
    kill_switch_enabled: bool,
) -> Vec<ActiveRule> {
    rules
        .unwrap_or_default()
        .iter()
        .filter_map(|rule| plan_rule(rule, kill_switch_enabled).ok().flatten())
        .collect()
}

/// One line per rule this monitor cannot act on, naming the rule and the reason.
fn unenforceable_rules(rules: &[RuleView], kill_switch_enabled: bool) -> Vec<String> {
    rules
        .iter()
        .filter_map(|rule| {
            plan_rule(rule, kill_switch_enabled)
                .err()
                .map(|reason| format!("rule '{}': {reason}", rule.id))
        })
        .collect()
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

    fn config(rules_yaml: &str) -> Vec<RuleView> {
        serde_yaml::from_str::<RuntimeMonitorView>(&format!("enabled: true\nrules:\n{rules_yaml}"))
            .expect("monitor config")
            .rules
    }

    fn read(policy: &str) -> MonitorPolicy {
        read_monitor_policy(policy.as_bytes()).expect("monitor reading")
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
        let rules = compile_active_rules(Some(&cfg[..]), KILL_SWITCH_ON);

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
            // A key only another rule type reads has no effect on this one.
            (
                "  - id: file-dest\n    type: file_open\n    match:\n      path_globs: ['/etc/**']\n      dest_globs: ['10.0.0.0/8']\n    action: deny\n",
                "rule 'file-dest': dest_globs has no effect on a file_open rule",
            ),
            (
                "  - id: exec-not-dest\n    type: proc_exec\n    match:\n      path_globs: ['**/sh']\n      not:\n        dest_globs: ['10.0.0.0/8']\n    action: log\n",
                "rule 'exec-not-dest': not.dest_globs has no effect on a proc_exec rule",
            ),
            (
                "  - id: file-not-not\n    type: file_open\n    match:\n      path_globs: ['/etc/**']\n      not:\n        path_globs: ['/etc/ok/**']\n        not:\n          path_globs: ['/etc/ok/x']\n    action: log\n",
                "rule 'file-not-not': not.not has no effect",
            ),
            (
                "  - id: net-path\n    type: net_connect\n    match:\n      dest_globs: ['10.0.0.0/8']\n      path_globs: ['/x']\n    action: deny\n",
                "rule 'net-path': path_globs has no effect on a net_connect rule",
            ),
            (
                "  - id: net-not\n    type: net_connect\n    match:\n      dest_globs: ['10.0.0.0/8']\n      not:\n        dest_globs: ['10.1.0.0/16']\n    action: deny\n",
                "rule 'net-not': not has no effect on a net_connect rule",
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
            assert!(compile_active_rules(Some(&config(yaml)[..]), KILL_SWITCH_ON).is_empty());
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
        assert!(compile_active_rules(Some(&config(EXEC_KILL)[..]), KILL_SWITCH_OFF).is_empty());

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
        let rules = compile_active_rules(Some(&config(two)[..]), KILL_SWITCH_ON);
        assert_eq!(
            matching_rule(EVENT_EXEC, "/usr/bin/curl", &rules).map(|r| r.id.as_str()),
            Some("first")
        );
    }

    /// "Enabled" is what the event handler reads when a rule fires: the section is present and its
    /// `enabled` is true, which is also the default for a section that does not say.
    #[test]
    fn the_kill_switch_is_enabled_only_when_present_and_not_switched_off() {
        let rules = "runtime_monitor:\n  enabled: true\n  rules:\n  - id: no-curl\n    type: proc_exec\n    match:\n      path_globs: ['**/curl']\n    action: trigger_kill\n";
        assert!(!read(rules).kill.enabled, "no kill_switch section");
        let off = read(&format!("kill_switch:\n  enabled: false\n{rules}"));
        assert!(!off.kill.enabled, "switched off");
        assert!(
            read(&format!("kill_switch:\n  enabled: true\n{rules}"))
                .kill
                .enabled
        );
        assert!(
            read(&format!("kill_switch: {{}}\n{rules}")).kill.enabled,
            "present, default on"
        );

        // And the rule that depends on it follows: present but switched off is refused.
        assert_eq!(off.refusals.len(), 1, "{:?}", off.refusals);
        assert!(off.refusals[0].starts_with("rule 'no-curl': trigger_kill on proc_exec needs"));
    }

    /// The view's defaults are the policy schema's, so a section that leaves a key out means the
    /// same to the monitor as it does to `policy resolve`.
    #[test]
    fn a_left_out_key_means_what_the_policy_schema_says() {
        use assay_core::mcp::runtime_features::{KillSwitchConfig, RuntimeMonitorConfig};
        let schema: KillSwitchConfig = serde_yaml::from_str("{}").expect("schema");
        let view: KillSwitchView = serde_yaml::from_str("{}").expect("view");
        assert_eq!(view.enabled, schema.enabled);
        assert_eq!(view.mode, schema.mode);
        assert_eq!(view.grace_period_ms, schema.grace_period_ms);
        assert!(view.triggers.is_empty() && schema.triggers.is_empty());
        let none = KillPlan::none();
        assert!(!none.enabled);
        assert_eq!(none.mode_for("any"), KillMode::Graceful);
        assert_eq!(none.grace_period_ms, schema.grace_period_ms);

        let schema: RuntimeMonitorConfig =
            serde_yaml::from_str("rules:\n  - id: r\n    type: file_open\n    match: {}\n")
                .expect("schema");
        let view: RuntimeMonitorView =
            serde_yaml::from_str("rules:\n  - id: r\n    type: file_open\n    match: {}\n")
                .expect("view");
        assert_eq!(view.enabled, schema.enabled);
        assert_eq!(view.rules[0].action, schema.rules[0].action);
    }

    /// A trigger sets the kill mode for its rule; every other rule uses the section's mode.
    #[test]
    fn a_trigger_sets_the_mode_for_its_rule_only() {
        let policy = read(
            "kill_switch:\n  mode: immediate\n  grace_period_ms: 250\n  triggers:\n    - on_rule: soft\n      mode: graceful\nruntime_monitor:\n  enabled: true\n  rules:\n  - id: soft\n    type: proc_exec\n    match:\n      path_globs: ['**/a']\n    action: trigger_kill\n  - id: hard\n    type: proc_exec\n    match:\n      path_globs: ['**/b']\n    action: trigger_kill\n",
        );
        assert!(policy.refusals.is_empty(), "{:?}", policy.refusals);
        assert_eq!(policy.kill.mode_for("soft"), KillMode::Graceful);
        assert_eq!(policy.kill.mode_for("hard"), KillMode::Immediate);
        assert_eq!(policy.kill.grace_period_ms, 250);
        assert_eq!(policy.rules.as_deref().map(<[RuleView]>::len), Some(2));
    }

    /// Each kind of trigger that has no effect is named once, and a working trigger is not.
    #[test]
    fn a_trigger_with_no_effect_is_refused() {
        let rules = "runtime_monitor:\n  enabled: true\n  rules:\n  - id: kill\n    type: file_open\n    match:\n      path_globs: ['/a']\n    action: trigger_kill\n  - id: log\n    type: file_open\n    match:\n      path_globs: ['/b']\n    action: log\n";
        let policy = read(&format!(
            "kill_switch:\n  triggers:\n    - on_rule: kill\n      mode: immediate\n    - on_rule: kill\n      mode: graceful\n    - on_rule: log\n      mode: immediate\n{rules}"
        ));
        assert_eq!(
            policy.refusals,
            [
                "kill_switch.triggers: rule 'kill' already has a trigger, and the first one is used, so this trigger has no effect",
                "kill_switch.triggers: rule 'log' does not use trigger_kill, so this trigger has no effect",
            ]
        );
        let no_mode = read(&format!(
            "kill_switch:\n  triggers:\n    - on_rule: kill\n{rules}"
        ));
        assert_eq!(
            no_mode.refusals,
            ["kill_switch.triggers: the trigger for rule 'kill' sets no mode, and kill_switch.mode applies without it, so this trigger has no effect"]
        );
        // No runtime_monitor section: no rule is there to trigger.
        let orphan =
            read("kill_switch:\n  triggers:\n    - on_rule: kill\n      mode: immediate\n");
        assert_eq!(
            orphan.refusals,
            ["kill_switch.triggers: rule 'kill' is not a runtime_monitor rule, so this trigger has no effect"]
        );
        assert!(orphan.rules.is_none());
    }

    /// The keys a view type takes, as serde sees them: the `fields` its derived `Deserialize` asks
    /// for, after renames. A probe deserializer records them and stops.
    fn keys_of<T: serde::de::DeserializeOwned>() -> Vec<&'static str> {
        #[derive(Debug)]
        struct Stop;
        impl std::fmt::Display for Stop {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("stop")
            }
        }
        impl std::error::Error for Stop {}
        impl serde::de::Error for Stop {
            fn custom<M: std::fmt::Display>(_: M) -> Self {
                Stop
            }
        }
        struct Probe<'a>(&'a mut Option<&'static [&'static str]>);
        impl<'de> serde::Deserializer<'de> for Probe<'_> {
            type Error = Stop;
            fn deserialize_any<V: serde::de::Visitor<'de>>(self, _: V) -> Result<V::Value, Stop> {
                Err(Stop)
            }
            fn deserialize_struct<V: serde::de::Visitor<'de>>(
                self,
                _: &'static str,
                fields: &'static [&'static str],
                _: V,
            ) -> Result<V::Value, Stop> {
                *self.0 = Some(fields);
                Err(Stop)
            }
            serde::forward_to_deserialize_any! {
                bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes
                byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct map enum
                identifier ignored_any
            }
        }
        let mut fields = None;
        let _ = T::deserialize(Probe(&mut fields));
        fields.expect("a struct with named fields").to_vec()
    }

    /// The guide's key table is the views, both ways: a key the monitor reads is documented, and a
    /// documented key is one the monitor reads.
    #[test]
    fn the_guide_lists_exactly_the_keys_the_monitor_reads() {
        use crate::cli::commands::monitor::monitor_next::view::{MatchView, TriggerView};
        let mut read = std::collections::BTreeSet::new();
        for (prefix, keys) in [
            ("runtime_monitor.", keys_of::<RuntimeMonitorView>()),
            ("runtime_monitor.rules[].", keys_of::<RuleView>()),
            ("runtime_monitor.rules[].match.", keys_of::<MatchView>()),
            ("kill_switch.", keys_of::<KillSwitchView>()),
            ("kill_switch.triggers[].", keys_of::<TriggerView>()),
        ] {
            read.extend(keys.into_iter().map(|key| format!("{prefix}{key}")));
        }
        let guide = include_str!("../../../../../../docs/guides/runtime-monitor.md");
        let documented: std::collections::BTreeSet<String> = guide
            .lines()
            .filter_map(|line| line.strip_prefix("| `"))
            .filter_map(|rest| rest.split_once("` |").map(|(key, _)| key))
            .filter(|key| key.starts_with("runtime_monitor.") || key.starts_with("kill_switch."))
            .map(str::to_string)
            .collect();
        assert_eq!(documented, read);
        assert!(
            read.contains("runtime_monitor.rules[].type"),
            "renames are applied: {read:?}"
        );
    }

    /// A disabled `runtime_monitor` is not read: the monitor exits before it acts on anything.
    #[test]
    fn a_disabled_monitor_reads_nothing_else() {
        let policy = read(
            "runtime_monitor:\n  enabled: false\n  scope: {}\nkill_switch:\n  kill_children: true\n",
        );
        assert!(policy.disabled);
        assert!(policy.refusals.is_empty(), "{:?}", policy.refusals);
        let absent = read("kill_switch:\n  kill_children: true\n");
        assert!(!absent.disabled && absent.rules.is_none());
        assert_eq!(
            absent.refusals,
            ["\"kill_switch.kill_children\": assay monitor does not act on this key; remove it"]
        );
    }
}
