use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ─────────────────────────────────────────────────────────────
// Discovery Config
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct DiscoveryConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default = "default_discovery_methods")]
    pub methods: Vec<DiscoveryMethod>,

    pub output: Option<PathBuf>,

    #[serde(default)]
    pub on_findings: DiscoveryActions,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryMethod {
    ConfigFiles,
    Processes,
    Network,
    Dns,
    WellKnown,
}

fn default_discovery_methods() -> Vec<DiscoveryMethod> {
    vec![DiscoveryMethod::ConfigFiles, DiscoveryMethod::Processes]
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct DiscoveryActions {
    #[serde(default)]
    pub unmanaged_server: ActionLevel,

    #[serde(default)]
    pub no_auth: ActionLevel,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionLevel {
    Log,
    Warn,
    Fail,
}

impl Default for ActionLevel {
    fn default() -> Self {
        ActionLevel::Log
    }
}

// ─────────────────────────────────────────────────────────────
// Runtime Monitor Config
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct RuntimeMonitorConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default)]
    pub provider: MonitorProvider,

    #[serde(default)]
    pub rules: Vec<MonitorRule>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MonitorProvider {
    Ebpf,
}

impl Default for MonitorProvider {
    fn default() -> Self {
        MonitorProvider::Ebpf
    }
}

/// If you already have a Severity type in assay-core: replace this with your existing Severity.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

impl Default for Severity {
    fn default() -> Self {
        Severity::Medium
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct MonitorRule {
    pub id: String,

    #[serde(rename = "type")]
    pub rule_type: MonitorRuleType,

    #[serde(rename = "match")]
    pub match_config: MonitorMatch,

    #[serde(default)]
    pub severity: Severity,

    #[serde(default)]
    pub action: MonitorAction,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MonitorRuleType {
    FileOpen,
    NetConnect,
    ProcExec,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct MonitorMatch {
    #[serde(default)]
    pub path_globs: Vec<String>,

    #[serde(default)]
    pub dest_globs: Vec<String>,

    #[serde(default)]
    pub not: Option<Box<MonitorMatch>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorAction {
    Log,
    Alert,
    TriggerKill,
}

impl Default for MonitorAction {
    fn default() -> Self {
        MonitorAction::Log
    }
}

// ─────────────────────────────────────────────────────────────
// Kill Switch Config
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct KillSwitchConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default)]
    pub mode: KillMode,

    #[serde(default = "default_grace_period")]
    pub grace_period_ms: u64,

    #[serde(default = "default_true")]
    pub kill_children: bool,

    #[serde(default)]
    pub capture_state: bool,

    pub output_dir: Option<PathBuf>,

    #[serde(default)]
    pub triggers: Vec<KillTrigger>,
}

impl Default for KillSwitchConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: KillMode::Graceful,
            grace_period_ms: default_grace_period(),
            kill_children: true,
            capture_state: false,
            output_dir: None,
            triggers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KillMode {
    Graceful,
    Immediate,
}

impl Default for KillMode {
    fn default() -> Self {
        KillMode::Graceful
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct KillTrigger {
    pub on_rule: String,

    #[serde(default)]
    pub mode: Option<KillMode>,
}

fn default_true() -> bool {
    true
}
fn default_grace_period() -> u64 {
    3000
}
