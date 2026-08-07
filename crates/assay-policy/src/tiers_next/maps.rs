use ipnet::IpNet;

use super::types::{InodeRule, Tier1Rules};

/// Actions the kernel `connect4` / `connect6` hooks branch on. These mirror
/// `assay_common::RULE_ACTION_*`, which this crate cannot import: `assay-policy`
/// is a leaf crate with no internal dependencies (see CLAUDE.md), and the loader
/// widens these to the shared `CidrRuleValue` at the userspace/eBPF boundary.
/// `network_egress_compile.rs` pins the values against that expectation.
const ACTION_ALLOW: u8 = 1;
const ACTION_DENY: u8 = 2;

/// One `CIDR_RULES_V4` entry: the LPM key plus the value the kernel hook reads.
///
/// `rule_id` is the whole reason this is a struct and not a `(prefix, addr, action)`
/// tuple: the hook reports it as the matched rule in `SocketEvent::rule_id`, so
/// dropping it here is what previously made every CIDR block claim the same rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CidrMapEntry {
    /// LPM prefix length in bits.
    pub prefix_len: u32,
    /// Network address, big-endian octets.
    pub addr: [u8; 4],
    /// `1` (allow) or `2` (deny), matching `assay_common::RULE_ACTION_*`.
    pub action: u8,
    /// Compiler-assigned id of the policy rule this entry came from.
    pub rule_id: u32,
}

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
    pub fn cidr_v4_entries(&self) -> Vec<CidrMapEntry> {
        let mut entries = Vec::new();

        for (rules, action) in [
            (&self.network_allow_cidrs, ACTION_ALLOW),
            (&self.network_deny_cidrs, ACTION_DENY),
        ] {
            for rule in rules {
                if let IpNet::V4(net) = rule.parsed {
                    entries.push(CidrMapEntry {
                        prefix_len: u32::from(net.prefix_len()),
                        addr: net.addr().octets(),
                        action,
                        rule_id: rule.rule_id,
                    });
                }
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
