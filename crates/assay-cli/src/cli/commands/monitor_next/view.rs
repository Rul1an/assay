//! The keys `assay monitor` reads from a policy, and nothing else.
//!
//! The policy schema (`assay_core::mcp::runtime_features`) is the union of what any Assay reader
//! accepts, with every default filled in, so it cannot say what the monitor acts on: it loaded
//! `kill_children: true` and `kill_scope: cgroup` while the monitor signalled the one process that
//! matched. These types hold only the keys the monitor acts on. Each section is read into them by
//! [`super::rules::read_monitor_policy`], and every key they do not take is refused there.
//!
//! The `forbid` below makes the compiler the check that each field here is acted on: a field that
//! nothing reads is a compile error, and no `allow` or `expect` inside this module can switch that
//! off. Adding a key the monitor does not act on fails the build; so does dropping the code that
//! read one. A read is not an effect, though: what the monitor does with a value is tested where it
//! does it.

#![forbid(dead_code)]

use assay_core::mcp::runtime_features::{KillMode, MonitorAction, MonitorRuleType};
use serde::Deserialize;

/// `runtime_monitor`.
#[derive(Debug, Deserialize)]
pub(crate) struct RuntimeMonitorView {
    /// Off unless the section says so, as in the policy schema.
    #[serde(default)]
    pub(crate) enabled: bool,
    #[serde(default)]
    pub(crate) rules: Vec<RuleView>,
}

/// One entry of `runtime_monitor.rules`.
#[derive(Debug, Deserialize)]
pub(crate) struct RuleView {
    pub(crate) id: String,
    #[serde(rename = "type")]
    pub(crate) kind: MonitorRuleType,
    #[serde(rename = "match")]
    pub(crate) matcher: MatchView,
    #[serde(default)]
    pub(crate) action: MonitorAction,
}

/// A rule's `match`, and its `not`. Which of these keys a rule acts on depends on its type, so the
/// rule's planner refuses the ones its type does not read.
#[derive(Debug, Deserialize)]
pub(crate) struct MatchView {
    #[serde(default)]
    pub(crate) path_globs: Vec<String>,
    #[serde(default)]
    pub(crate) dest_globs: Vec<String>,
    #[serde(default)]
    pub(crate) not: Option<Box<MatchView>>,
}

/// `kill_switch`.
#[derive(Debug, Deserialize)]
pub(crate) struct KillSwitchView {
    /// On when the section is present and does not say, as in the policy schema.
    #[serde(default = "enabled_by_default")]
    pub(crate) enabled: bool,
    #[serde(default)]
    pub(crate) mode: KillMode,
    #[serde(default = "default_grace_period_ms")]
    pub(crate) grace_period_ms: u64,
    #[serde(default)]
    pub(crate) triggers: Vec<TriggerView>,
}

/// One entry of `kill_switch.triggers`.
#[derive(Debug, Deserialize)]
pub(crate) struct TriggerView {
    pub(crate) on_rule: String,
    #[serde(default)]
    pub(crate) mode: Option<KillMode>,
}

fn enabled_by_default() -> bool {
    true
}

fn default_grace_period_ms() -> u64 {
    3000
}
