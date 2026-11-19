use assay_common::closed;
use schemars::JsonSchema;
use serde::Serialize;
use std::path::PathBuf;

// ─────────────────────────────────────────────────────────────
// Discovery Config
// ─────────────────────────────────────────────────────────────

closed! {
    #[derive(Serialize, JsonSchema, Default)]
    pub struct DiscoveryConfig {
        #[default]
        enabled: bool,

        #[default = "default_discovery_methods"]
        methods: Vec<DiscoveryMethod>,

        output: Option<PathBuf>,

        #[default]
        on_findings: DiscoveryActions,
    }
}

closed! {
    #[derive(Serialize, JsonSchema)]
    pub enum DiscoveryMethod {
        "config_files" => ConfigFiles,
        "processes" => Processes,
        "network" => Network,
        "dns" => Dns,
        "well_known" => WellKnown,
    }
}

fn default_discovery_methods() -> Vec<DiscoveryMethod> {
    vec![DiscoveryMethod::ConfigFiles, DiscoveryMethod::Processes]
}

closed! {
    #[derive(Serialize, JsonSchema, Default)]
    pub struct DiscoveryActions {
        #[default]
        unmanaged_server: ActionLevel,

        #[default]
        no_auth: ActionLevel,
    }
}

closed! {
    #[derive(Serialize, JsonSchema, Eq, Default)]
    pub enum ActionLevel {
        #[default]
        "log" => Log,
        "warn" => Warn,
        "fail" => Fail,
    }
}

// ─────────────────────────────────────────────────────────────
// Runtime Monitor Config
// ─────────────────────────────────────────────────────────────

closed! {
    #[derive(Serialize, JsonSchema, Default)]
    pub struct RuntimeMonitorConfig {
        #[default]
        enabled: bool,

        #[default]
        provider: MonitorProvider,

        /// SOTA Phase 5: Cgroup Scope Configuration
        #[default]
        scope: MonitorScopeConfig,

        #[default]
        rules: Vec<MonitorRule>,
    }
}

closed! {
    #[derive(Serialize, JsonSchema)]
    pub struct MonitorScopeConfig {
        #[default]
        mode: MonitorMode,

        #[default]
        cgroup: CgroupConfig,
    }
}

impl Default for MonitorScopeConfig {
    fn default() -> Self {
        Self {
            mode: MonitorMode::CgroupV2,
            cgroup: CgroupConfig::default(),
        }
    }
}

closed! {
    #[derive(Serialize, JsonSchema, Eq, Default)]
    pub enum MonitorMode {
        #[default]
        "cgroup_v2" => CgroupV2,
        "pid_set" => PidSet, // Legacy
    }
}

closed! {
    #[derive(Serialize, JsonSchema)]
    pub struct CgroupConfig {
        #[default = "default_true"]
        freeze_on_incident: bool,

        #[default = "default_true"]
        create_leaf: bool,

        #[default = "default_assay_prefix"]
        name_prefix: String,

        #[default = "default_true"]
        cleanup: bool,

        #[default = "default_pids_max"]
        pids_max: u32,
    }
}

impl Default for CgroupConfig {
    fn default() -> Self {
        Self {
            freeze_on_incident: true,
            create_leaf: true,
            name_prefix: default_assay_prefix(),
            cleanup: true,
            pids_max: default_pids_max(),
        }
    }
}

fn default_assay_prefix() -> String {
    "assay".to_string()
}

fn default_pids_max() -> u32 {
    2048
}

closed! {
    #[derive(Serialize, JsonSchema, Default)]
    pub enum MonitorProvider {
        #[default]
        "ebpf" => Ebpf,
    }
}

closed! {
    /// If you already have a Severity type in assay-core: replace this with your existing Severity.
    #[derive(Serialize, JsonSchema, Default)]
    pub enum Severity {
        "low" => Low,
        #[default]
        "medium" => Medium,
        "high" => High,
        "critical" => Critical,
    }
}

closed! {
    #[derive(Serialize, JsonSchema)]
    pub struct MonitorRule {
        id: String,

        #[key = "type"]
        rule_type: MonitorRuleType,

        #[key = "match"]
        match_config: MonitorMatch,

        #[default]
        severity: Severity,

        #[default]
        action: MonitorAction,
    }
}

closed! {
    #[derive(Serialize, JsonSchema)]
    pub enum MonitorRuleType {
        "file_open" => FileOpen,
        "net_connect" => NetConnect,
        "proc_exec" => ProcExec,
    }
}

closed! {
    #[derive(Serialize, JsonSchema, Default)]
    pub struct MonitorMatch {
        #[default]
        path_globs: Vec<String>,

        #[default]
        dest_globs: Vec<String>,

        #[default]
        not: Option<Box<MonitorMatch>>,
    }
}

closed! {
    #[derive(Serialize, JsonSchema, Eq, Default)]
    pub enum MonitorAction {
        #[default]
        "log" => Log,
        "alert" => Alert,
        "deny" => Deny,
        "trigger_kill" => TriggerKill,
    }
}

// ─────────────────────────────────────────────────────────────
// Kill Switch Config
// ─────────────────────────────────────────────────────────────

closed! {
    #[derive(Serialize, JsonSchema)]
    pub struct KillSwitchConfig {
        #[default = "default_true"]
        enabled: bool,

        #[default]
        mode: KillMode,

        #[default]
        kill_scope: KillScope,

        #[default = "default_grace_period"]
        grace_period_ms: u64,

        #[default = "default_true"]
        kill_children: bool,

        #[default]
        capture_state: bool,

        output_dir: Option<PathBuf>,

        #[default]
        triggers: Vec<KillTrigger>,
    }
}

impl Default for KillSwitchConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: KillMode::Graceful,
            kill_scope: KillScope::default(),
            grace_period_ms: default_grace_period(),
            kill_children: true,
            capture_state: false,
            output_dir: None,
            triggers: Vec::new(),
        }
    }
}

closed! {
    #[derive(Serialize, JsonSchema, Eq, Default)]
    pub enum KillMode {
        #[default]
        "graceful" => Graceful,
        "immediate" => Immediate,
    }
}

closed! {
    #[derive(Serialize, JsonSchema, Eq, Default)]
    pub enum KillScope {
        #[default]
        "cgroup" => Cgroup,
        "pid_fd" => PidFd,
        "legacy_pid" => LegacyPid,
    }
}

closed! {
    #[derive(Serialize, JsonSchema)]
    pub struct KillTrigger {
        on_rule: String,

        #[default]
        mode: Option<KillMode>,
    }
}

fn default_true() -> bool {
    true
}
fn default_grace_period() -> u64 {
    3000
}
