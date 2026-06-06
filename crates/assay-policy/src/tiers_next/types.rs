use ipnet::IpNet;
use serde::{Deserialize, Serialize};

/// Original user-facing policy
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Policy {
    #[serde(default)]
    pub files: FilePolicy,

    #[serde(default)]
    pub network: NetworkPolicy,

    #[serde(default)]
    pub processes: ProcessPolicy,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FilePolicy {
    #[serde(default)]
    pub deny: Vec<String>,

    #[serde(default)]
    pub allow: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NetworkPolicy {
    #[serde(default)]
    pub allow_cidrs: Vec<String>,

    #[serde(default)]
    pub deny_cidrs: Vec<String>,

    #[serde(default)]
    pub allow_ports: Vec<u16>,

    #[serde(default)]
    pub deny_ports: Vec<u16>,

    #[serde(default)]
    pub deny_destinations: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProcessPolicy {
    #[serde(default)]
    pub deny_executables: Vec<String>,

    #[serde(default)]
    pub allow_executables: Vec<String>,
}

/// Compiled policy split into tiers
#[derive(Debug, Clone, Serialize)]
pub struct CompiledPolicy {
    /// Tier 1: Kernel-enforced rules (LSM)
    pub tier1: Tier1Rules,

    /// Tier 2: Userspace-detected rules (Tracepoints)
    pub tier2: Tier2Rules,

    /// Compilation stats
    pub stats: CompilationStats,
}

/// Tier 1: Simple rules for kernel enforcement
#[derive(Debug, Clone, Default, Serialize)]
pub struct Tier1Rules {
    /// Exact path deny list (hash-matched in kernel)
    pub file_deny_exact: Vec<PathRule>,

    /// Prefix deny list (substring match in kernel)
    pub file_deny_prefix: Vec<PathRule>,

    /// CIDR allow list (LPM trie)
    pub network_allow_cidrs: Vec<CidrRule>,

    /// CIDR deny list
    pub network_deny_cidrs: Vec<CidrRule>,

    /// Port deny list
    pub network_deny_ports: Vec<PortRule>,

    /// Port allow list (bypass CIDR check)
    pub network_allow_ports: Vec<u16>,

    /// Inode exact deny list (SOTA)
    pub inode_deny_exact: Vec<InodeRule>,
}

/// Tier 2: Complex rules for userspace evaluation
#[derive(Debug, Clone, Default, Serialize)]
pub struct Tier2Rules {
    /// Glob patterns for file deny
    pub file_deny_globs: Vec<GlobRule>,

    /// Glob patterns for file allow (exceptions)
    pub file_allow_globs: Vec<GlobRule>,

    /// Destination patterns (host:port globs)
    pub network_deny_destinations: Vec<DestRule>,

    /// Process executable patterns
    pub process_deny_globs: Vec<GlobRule>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PathRule {
    pub rule_id: u32,
    pub path: String,
    pub hash: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CidrRule {
    pub rule_id: u32,
    pub cidr: String,
    pub parsed: IpNet,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortRule {
    pub rule_id: u32,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize)]
pub struct InodeRule {
    pub rule_id: u32,
    pub dev: u32,
    pub ino: u64,
    pub gen: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobRule {
    pub rule_id: u32,
    pub pattern: String,
    pub original: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DestRule {
    pub rule_id: u32,
    pub pattern: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CompilationStats {
    pub total_rules: usize,
    pub tier1_rules: usize,
    pub tier2_rules: usize,
    pub warnings: Vec<String>,
}
