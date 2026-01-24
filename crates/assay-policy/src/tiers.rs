// ============================================================================
// FILE: crates/assay-policy/src/tiers.rs
// Policy Tier Compiler - splits policy into enforcement layers
// ============================================================================

//! Policy compilation into enforcement tiers.
//!
//! # Tier Model
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                         User Policy (YAML)                              │
//! │  files:                                                                 │
//! │    deny: ["/etc/shadow", "**/.ssh/id_*", "/tmp/**/*.sh"]               │
//! │  network:                                                               │
//! │    allow: ["10.0.0.0/8", "192.168.0.0/16"]                             │
//! │    deny_ports: [22, 23, 3389]                                          │
//! └───────────────────────────────┬─────────────────────────────────────────┘
//!                                 │
//!                                 ▼
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                       TIER COMPILER                                      │
//! │  1. Classify each rule by complexity                                    │
//! │  2. Simple patterns → Tier 1 (LSM, kernel blocking)                    │
//! │  3. Complex patterns → Tier 2 (Tracepoints, userspace)                 │
//! └───────────────────────────────┬─────────────────────────────────────────┘
//!                                 │
//!                 ┌───────────────┴───────────────┐
//!                 ▼                               ▼
//! ┌───────────────────────────┐   ┌───────────────────────────────┐
//! │   TIER 1: LSM (Kernel)    │   │  TIER 2: Tracepoints (User)   │
//! │                           │   │                               │
//! │ • Exact paths             │   │ • Glob patterns (**/*)        │
//! │ • Simple prefixes         │   │ • Regex patterns              │
//! │ • CIDR blocks             │   │ • Complex conditions          │
//! │ • Port numbers            │   │ • Rate limiting               │
//! │                           │   │ • Correlation rules           │
//! │ ✓ PREVENTS access         │   │ ✓ DETECTS + reactive kill    │
//! │ ✓ Zero TOCTOU             │   │ ⚠ Small race window          │
//! └───────────────────────────┘   └───────────────────────────────┘
//! ```
//!
//! # Why Split?
//!
//! - **Verifier limits**: eBPF verifier can't handle complex glob matching
//! - **Performance**: Simple checks in kernel, complex in userspace
//! - **Defense in depth**: Critical paths blocked by LSM, others detected

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

// ─────────────────────────────────────────────────────────────────────────────
// Compiler
// ─────────────────────────────────────────────────────────────────────────────

/// Compile a policy into enforcement tiers
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn compile(policy: &Policy) -> CompiledPolicy {
    let mut tier1 = Tier1Rules::default();
    let mut tier2 = Tier2Rules::default();
    let mut stats = CompilationStats::default();
    let mut rule_id = 1u32;

    // ─────────────────────────────────────────────────────────────────────────
    // File rules
    // ─────────────────────────────────────────────────────────────────────────

    for pattern in &policy.files.deny {
        stats.total_rules += 1;

        match classify_path_pattern(pattern) {
            PathClass::Exact => {
                tier1.file_deny_exact.push(PathRule {
                    rule_id,
                    path: pattern.clone(),
                    hash: fnv1a_hash(pattern.as_bytes()),
                });
                stats.tier1_rules += 1;
            }
            PathClass::Prefix(prefix) => {
                tier1.file_deny_prefix.push(PathRule {
                    rule_id,
                    path: prefix.clone(),
                    hash: fnv1a_hash(prefix.as_bytes()),
                });
                stats.tier1_rules += 1;
            }
            PathClass::Glob => {
                tier2.file_deny_globs.push(GlobRule {
                    rule_id,
                    pattern: pattern.clone(),
                    original: pattern.clone(),
                });
                stats.tier2_rules += 1;
            }
        }

        rule_id += 1;
    }

    for pattern in &policy.files.allow {
        stats.total_rules += 1;

        // Allow rules always go to Tier 2 (userspace can handle exceptions)
        tier2.file_allow_globs.push(GlobRule {
            rule_id,
            pattern: pattern.clone(),
            original: pattern.clone(),
        });
        stats.tier2_rules += 1;
        rule_id += 1;
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Network rules
    // ─────────────────────────────────────────────────────────────────────────

    // CIDR rules → Tier 1 (LPM trie)
    for cidr_str in &policy.network.allow_cidrs {
        stats.total_rules += 1;

        match cidr_str.parse::<IpNet>() {
            Ok(cidr) => {
                tier1.network_allow_cidrs.push(CidrRule {
                    rule_id,
                    cidr: cidr_str.clone(),
                    parsed: cidr,
                });
                stats.tier1_rules += 1;
            }
            Err(e) => {
                stats
                    .warnings
                    .push(format!("Invalid CIDR '{cidr_str}': {e}"));
            }
        }
        rule_id += 1;
    }

    for cidr_str in &policy.network.deny_cidrs {
        stats.total_rules += 1;

        match cidr_str.parse::<IpNet>() {
            Ok(cidr) => {
                tier1.network_deny_cidrs.push(CidrRule {
                    rule_id,
                    cidr: cidr_str.clone(),
                    parsed: cidr,
                });
                stats.tier1_rules += 1;
            }
            Err(e) => {
                stats
                    .warnings
                    .push(format!("Invalid CIDR '{cidr_str}': {e}"));
            }
        }
        rule_id += 1;
    }

    // Port rules → Tier 1
    for port in &policy.network.deny_ports {
        stats.total_rules += 1;
        tier1.network_deny_ports.push(PortRule {
            rule_id,
            port: *port,
        });
        stats.tier1_rules += 1;
        rule_id += 1;
    }

    tier1
        .network_allow_ports
        .clone_from(&policy.network.allow_ports);

    // Destination patterns → Tier 2
    for dest in &policy.network.deny_destinations {
        stats.total_rules += 1;
        tier2.network_deny_destinations.push(DestRule {
            rule_id,
            pattern: dest.clone(),
        });
        stats.tier2_rules += 1;
        rule_id += 1;
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Process rules
    // ─────────────────────────────────────────────────────────────────────────

    for pattern in &policy.processes.deny_executables {
        stats.total_rules += 1;

        // Process rules → Tier 2 (exec monitoring is best-effort anyway)
        tier2.process_deny_globs.push(GlobRule {
            rule_id,
            pattern: pattern.clone(),
            original: pattern.clone(),
        });
        stats.tier2_rules += 1;
        rule_id += 1;
    }

    CompiledPolicy {
        tier1,
        tier2,
        stats,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Pattern Classification
// ─────────────────────────────────────────────────────────────────────────────

enum PathClass {
    /// Exact path (no wildcards)
    Exact,
    /// Prefix match (ends with /*)
    Prefix(String),
    /// Complex glob (**, ?, etc.)
    Glob,
}

fn classify_path_pattern(pattern: &str) -> PathClass {
    // Check for glob characters
    let has_double_star = pattern.contains("**");
    let has_single_star = pattern.contains('*');
    let has_question = pattern.contains('?');
    let has_bracket = pattern.contains('[');

    if !has_single_star && !has_question && !has_bracket {
        // No wildcards - exact match
        return PathClass::Exact;
    }

    if has_double_star || has_question || has_bracket {
        // Complex pattern - needs userspace
        return PathClass::Glob;
    }

    // Single star - might be a simple prefix
    if pattern.ends_with("/*") && pattern.matches('*').count() == 1 {
        // Pattern like "/home/user/*" - treat as prefix
        let prefix = &pattern[..pattern.len() - 1]; // Remove trailing *
        return PathClass::Prefix(prefix.to_string());
    }

    // Pattern like "/etc/*.conf" - needs glob matching
    PathClass::Glob
}

/// FNV-1a hash (same as kernel)
fn fnv1a_hash(data: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0100_0000_01b3;

    let mut hash = FNV_OFFSET;
    for &byte in data {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

// ─────────────────────────────────────────────────────────────────────────────
// BPF Map Population
// ─────────────────────────────────────────────────────────────────────────────

impl Tier1Rules {
    /// Generate entries for `DENY_PATHS_EXACT` map
    #[must_use]
    pub fn file_exact_entries(&self) -> Vec<(u64, u32)> {
        self.file_deny_exact
            .iter()
            .map(|r| (r.hash, r.rule_id))
            .collect()
    }

    /// Generate entries for `DENY_PATHS_PREFIX` map
    #[must_use]
    pub fn file_prefix_entries(&self) -> Vec<(u64, (u32, u32))> {
        self.file_deny_prefix
            .iter()
            .map(|r| {
                (
                    r.hash,
                    (u32::try_from(r.path.len()).unwrap_or(0), r.rule_id),
                )
            })
            .collect()
    }

    /// Generate entries for `CIDR_RULES_V4` map
    #[must_use]
    pub fn cidr_v4_entries(&self) -> Vec<(u32, [u8; 4], u8)> {
        let mut entries = Vec::new();

        // Allow rules (action = 1)
        for rule in &self.network_allow_cidrs {
            if let IpNet::V4(net) = rule.parsed {
                entries.push((
                    u32::from(net.prefix_len()),
                    net.addr().octets(),
                    1, // ACTION_ALLOW
                ));
            }
        }

        // Deny rules (action = 2)
        for rule in &self.network_deny_cidrs {
            if let IpNet::V4(net) = rule.parsed {
                entries.push((
                    u32::from(net.prefix_len()),
                    net.addr().octets(),
                    2, // ACTION_DENY
                ));
            }
        }

        entries
    }

    /// Generate entries for `DENY_PORTS` map
    #[must_use]
    pub fn port_deny_entries(&self) -> Vec<(u16, u32)> {
        self.network_deny_ports
            .iter()
            .map(|r| (r.port, r.rule_id))
            .collect()
    }

    /// Generate entries for `DENY_INO` map (SOTA)
    #[must_use]
    pub fn inode_exact_entries(&self) -> Vec<(String, InodeRule)> {
        self.inode_deny_exact
            .iter()
            .map(|r| (format!("{}:{}", r.dev, r.ino), r.clone()))
            .collect()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_exact() {
        assert!(matches!(
            classify_path_pattern("/etc/shadow"),
            PathClass::Exact
        ));
        assert!(matches!(
            classify_path_pattern("/home/user/.ssh/id_rsa"),
            PathClass::Exact
        ));
    }

    #[test]
    fn test_classify_prefix() {
        match classify_path_pattern("/home/user/*") {
            PathClass::Prefix(p) => assert_eq!(p, "/home/user/"),
            _ => panic!("Expected Prefix"),
        }
    }

    #[test]
    fn test_classify_glob() {
        assert!(matches!(
            classify_path_pattern("**/.ssh/*"),
            PathClass::Glob
        ));
        assert!(matches!(
            classify_path_pattern("/etc/*.conf"),
            PathClass::Glob
        ));
        assert!(matches!(
            classify_path_pattern("/tmp/file?.txt"),
            PathClass::Glob
        ));
    }

    #[test]
    fn test_compile_splits_tiers() {
        let policy = Policy {
            files: FilePolicy {
                deny: vec![
                    "/etc/shadow".to_string(),  // → Tier 1 exact
                    "/home/user/*".to_string(), // → Tier 1 prefix
                    "**/.ssh/id_*".to_string(), // → Tier 2 glob
                ],
                allow: vec![],
            },
            network: NetworkPolicy {
                allow_cidrs: vec!["10.0.0.0/8".to_string()],
                deny_ports: vec![22, 23],
                ..Default::default()
            },
            processes: ProcessPolicy::default(),
        };

        let compiled = compile(&policy);

        // File rules
        assert_eq!(compiled.tier1.file_deny_exact.len(), 1);
        assert_eq!(compiled.tier1.file_deny_prefix.len(), 1);
        assert_eq!(compiled.tier2.file_deny_globs.len(), 1);

        // Network rules
        assert_eq!(compiled.tier1.network_allow_cidrs.len(), 1);
        assert_eq!(compiled.tier1.network_deny_ports.len(), 2);

        // Stats
        assert_eq!(compiled.stats.tier1_rules, 5); // 1 exact + 1 prefix + 1 cidr + 2 ports
        assert_eq!(compiled.stats.tier2_rules, 1); // 1 glob
    }

    #[test]
    fn test_hash_consistency() {
        // Ensure hash is consistent (for kernel matching)
        let hash1 = fnv1a_hash(b"/etc/shadow");
        let hash2 = fnv1a_hash(b"/etc/shadow");
        assert_eq!(hash1, hash2);

        let hash3 = fnv1a_hash(b"/etc/passwd");
        assert_ne!(hash1, hash3);
    }
}
