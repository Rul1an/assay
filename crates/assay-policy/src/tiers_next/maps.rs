use ipnet::IpNet;

use super::types::{InodeRule, Tier1Rules};

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

        // Allow rules (action = 1).
        for rule in &self.network_allow_cidrs {
            if let IpNet::V4(net) = rule.parsed {
                entries.push((
                    u32::from(net.prefix_len()),
                    net.addr().octets(),
                    1, // ACTION_ALLOW
                ));
            }
        }

        // Deny rules (action = 2).
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
