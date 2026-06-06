use ipnet::IpNet;

use super::classifier::{classify_path_pattern, fnv1a_hash, PathClass};
use super::types::{
    CidrRule, CompilationStats, CompiledPolicy, DestRule, FilePolicy, GlobRule, NetworkPolicy,
    PathRule, Policy, PortRule, ProcessPolicy, Tier1Rules, Tier2Rules,
};

/// Compile a policy into enforcement tiers
#[must_use]
pub fn compile(policy: &Policy) -> CompiledPolicy {
    let mut tier1 = Tier1Rules::default();
    let mut tier2 = Tier2Rules::default();
    let mut stats = CompilationStats::default();
    let mut rule_id = 1u32;

    compile_file_rules(
        &policy.files,
        &mut tier1,
        &mut tier2,
        &mut stats,
        &mut rule_id,
    );
    compile_network_rules(
        &policy.network,
        &mut tier1,
        &mut tier2,
        &mut stats,
        &mut rule_id,
    );
    compile_process_rules(&policy.processes, &mut tier2, &mut stats, &mut rule_id);

    CompiledPolicy {
        tier1,
        tier2,
        stats,
    }
}

fn compile_file_rules(
    policy: &FilePolicy,
    tier1: &mut Tier1Rules,
    tier2: &mut Tier2Rules,
    stats: &mut CompilationStats,
    rule_id: &mut u32,
) {
    for pattern in &policy.deny {
        stats.total_rules += 1;

        match classify_path_pattern(pattern) {
            PathClass::Exact => {
                tier1.file_deny_exact.push(PathRule {
                    rule_id: *rule_id,
                    path: pattern.clone(),
                    hash: fnv1a_hash(pattern.as_bytes()),
                });
                stats.tier1_rules += 1;
            }
            PathClass::Prefix(prefix) => {
                tier1.file_deny_prefix.push(PathRule {
                    rule_id: *rule_id,
                    path: prefix.clone(),
                    hash: fnv1a_hash(prefix.as_bytes()),
                });
                stats.tier1_rules += 1;
            }
            PathClass::Glob => {
                tier2.file_deny_globs.push(GlobRule {
                    rule_id: *rule_id,
                    pattern: pattern.clone(),
                    original: pattern.clone(),
                });
                stats.tier2_rules += 1;
            }
        }

        *rule_id += 1;
    }

    for pattern in &policy.allow {
        stats.total_rules += 1;

        // Allow rules always go to Tier 2 (userspace can handle exceptions).
        tier2.file_allow_globs.push(GlobRule {
            rule_id: *rule_id,
            pattern: pattern.clone(),
            original: pattern.clone(),
        });
        stats.tier2_rules += 1;
        *rule_id += 1;
    }
}

fn compile_network_rules(
    policy: &NetworkPolicy,
    tier1: &mut Tier1Rules,
    tier2: &mut Tier2Rules,
    stats: &mut CompilationStats,
    rule_id: &mut u32,
) {
    // CIDR rules go to Tier 1 (LPM trie).
    for cidr_str in &policy.allow_cidrs {
        stats.total_rules += 1;

        match cidr_str.parse::<IpNet>() {
            Ok(cidr) => {
                tier1.network_allow_cidrs.push(CidrRule {
                    rule_id: *rule_id,
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
        *rule_id += 1;
    }

    for cidr_str in &policy.deny_cidrs {
        stats.total_rules += 1;

        match cidr_str.parse::<IpNet>() {
            Ok(cidr) => {
                tier1.network_deny_cidrs.push(CidrRule {
                    rule_id: *rule_id,
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
        *rule_id += 1;
    }

    // Port rules go to Tier 1.
    for port in &policy.deny_ports {
        stats.total_rules += 1;
        tier1.network_deny_ports.push(PortRule {
            rule_id: *rule_id,
            port: *port,
        });
        stats.tier1_rules += 1;
        *rule_id += 1;
    }

    tier1.network_allow_ports.clone_from(&policy.allow_ports);

    // Destination patterns go to Tier 2.
    for dest in &policy.deny_destinations {
        stats.total_rules += 1;
        tier2.network_deny_destinations.push(DestRule {
            rule_id: *rule_id,
            pattern: dest.clone(),
        });
        stats.tier2_rules += 1;
        *rule_id += 1;
    }
}

fn compile_process_rules(
    policy: &ProcessPolicy,
    tier2: &mut Tier2Rules,
    stats: &mut CompilationStats,
    rule_id: &mut u32,
) {
    for pattern in &policy.deny_executables {
        stats.total_rules += 1;

        // Process rules go to Tier 2 (exec monitoring is best-effort anyway).
        tier2.process_deny_globs.push(GlobRule {
            rule_id: *rule_id,
            pattern: pattern.clone(),
            original: pattern.clone(),
        });
        stats.tier2_rules += 1;
        *rule_id += 1;
    }
}
