mod error;
pub use error::MonitorError;

pub mod events;
pub mod tree;

#[cfg(target_os = "linux")]
mod loader;
#[cfg(target_os = "linux")]
pub mod tracepoint;

use assay_common::MonitorEvent;

// We use the alias from events, or define it here.
pub type EventStream = tokio_stream::wrappers::ReceiverStream<Result<MonitorEvent, MonitorError>>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MonitorStatsSnapshot {
    pub tracepoint_events_emitted: u32,
    pub tracepoint_ringbuf_dropped: u32,
    pub lsm_events_emitted: u32,
    pub lsm_ringbuf_dropped: u32,
    pub openat_events_emitted: u32,
    pub openat_ringbuf_dropped: u32,
    pub openat2_events_emitted: u32,
    pub openat2_ringbuf_dropped: u32,
    pub connect_events_emitted: u32,
    pub connect_ringbuf_dropped: u32,
    pub sendto_events_emitted: u32,
    pub sendto_ringbuf_dropped: u32,
    pub sendmsg_events_emitted: u32,
    pub sendmsg_ringbuf_dropped: u32,
    pub sendto_no_peer: u32,
    pub sendmsg_no_peer: u32,
    pub sendto_non_ip_family: u32,
    pub sendmsg_non_ip_family: u32,
    pub socket_checks: u64,
    pub socket_blocked_cidr: u64,
    pub socket_blocked_port: u64,
    pub socket_allowed: u64,
    pub socket_events_emitted: u64,
    pub socket_ringbuf_dropped: u64,
    /// Userspace count of ring-buffer records rejected because their length did not match the
    /// pinned `MonitorEvent` size. A non-zero value almost always means a stale eBPF object
    /// (built from an older `MonitorEvent` layout) was loaded against a newer userspace decoder;
    /// the records are dropped fail-closed, so this is an observation gap, not decoded garbage.
    pub event_size_mismatch: u64,
}

impl MonitorStatsSnapshot {
    pub fn total_ringbuf_dropped(&self) -> u64 {
        u64::from(self.tracepoint_ringbuf_dropped)
            + u64::from(self.lsm_ringbuf_dropped)
            + self.socket_ringbuf_dropped
    }

    pub fn has_ringbuf_pressure(&self) -> bool {
        self.total_ringbuf_dropped() > 0
    }
}

pub struct Monitor {
    #[cfg(target_os = "linux")]
    inner: loader::LinuxMonitor,

    #[cfg(not(target_os = "linux"))]
    _stub: (),
}

/// Validate network rules against the enforcement target shipped by `assay-monitor`.
///
/// The policy compiler accepts both address families, while the userspace loader currently
/// populates only `CIDR_RULES_V4` and attaches only `connect4_hook`. Refuse IPv6 here so a valid
/// policy can never be partially projected into IPv4 maps and then reported as enforced.
pub fn validate_network_enforcement_support(
    compiled: &assay_policy::tiers::CompiledPolicy,
) -> Result<(), MonitorError> {
    let ipv6_allow = compiled
        .tier1
        .network_allow_cidrs
        .iter()
        .filter(|rule| rule.parsed.addr().is_ipv6())
        .count();
    let ipv6_deny = compiled
        .tier1
        .network_deny_cidrs
        .iter()
        .filter(|rule| rule.parsed.addr().is_ipv6())
        .count();

    if ipv6_allow != 0 || ipv6_deny != 0 {
        return Err(MonitorError::EnforcementUnavailable(format!(
            "the current enforcement target supports IPv4/TCP only; policy contains {ipv6_allow} \
             IPv6 allow CIDR rule(s) and {ipv6_deny} IPv6 deny CIDR rule(s)"
        )));
    }

    Ok(())
}

impl Monitor {
    /// Load eBPF object bytes from file (Linux). Non-Linux returns NotSupported.
    pub fn load_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, MonitorError> {
        #[cfg(target_os = "linux")]
        {
            let inner = loader::LinuxMonitor::load_file(path)?;
            Ok(Self { inner })
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = path;
            Err(MonitorError::NotSupported)
        }
    }

    /// Load eBPF object bytes from memory (Linux). Non-Linux returns NotSupported.
    pub fn load_bytes(bytes: &[u8]) -> Result<Self, MonitorError> {
        #[cfg(target_os = "linux")]
        {
            let inner = loader::LinuxMonitor::load_bytes(bytes)?;
            Ok(Self { inner })
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = bytes;
            Err(MonitorError::NotSupported)
        }
    }

    /// Configure monitored PIDs by writing to MONITORED_PIDS map.
    pub fn set_monitored_pids(&mut self, pids: &[u32]) -> Result<(), MonitorError> {
        #[cfg(target_os = "linux")]
        return self.inner.set_monitored_pids(pids);

        #[cfg(not(target_os = "linux"))]
        {
            let _ = pids;
            Err(MonitorError::NotSupported)
        }
    }

    pub fn configure_defaults(&mut self) -> Result<(), MonitorError> {
        #[cfg(target_os = "linux")]
        return self.inner.configure_defaults();

        #[cfg(not(target_os = "linux"))]
        Ok(())
    }

    pub fn set_monitored_cgroups(&mut self, cgroups: &[u64]) -> Result<(), MonitorError> {
        #[cfg(target_os = "linux")]
        return self.inner.set_monitored_cgroups(cgroups);

        #[cfg(not(target_os = "linux"))]
        {
            let _ = cgroups;
            Err(MonitorError::NotSupported)
        }
    }

    pub fn set_tier1_rules(
        &mut self,
        compiled: &assay_policy::tiers::CompiledPolicy,
    ) -> Result<(), MonitorError> {
        validate_network_enforcement_support(compiled)?;

        #[cfg(target_os = "linux")]
        return self.inner.set_tier1_rules(compiled);

        #[cfg(not(target_os = "linux"))]
        {
            let _ = compiled;
            Ok(())
        }
    }

    pub fn attach_network_cgroup(
        &mut self,
        cgroup_file: &std::fs::File,
    ) -> Result<(), MonitorError> {
        #[cfg(target_os = "linux")]
        return self.inner.attach_network_cgroup(cgroup_file);

        #[cfg(not(target_os = "linux"))]
        {
            let _ = cgroup_file;
            Err(MonitorError::NotSupported)
        }
    }

    pub fn set_monitor_all(&mut self, enabled: bool) -> Result<(), MonitorError> {
        #[cfg(target_os = "linux")]
        return self.inner.set_monitor_all(enabled);

        #[cfg(not(target_os = "linux"))]
        {
            let _ = enabled;
            Ok(())
        }
    }

    pub fn set_emit_inode_resolved(&mut self, enabled: bool) -> Result<(), MonitorError> {
        #[cfg(target_os = "linux")]
        return self.inner.set_emit_inode_resolved(enabled);

        #[cfg(not(target_os = "linux"))]
        {
            let _ = enabled;
            Ok(())
        }
    }

    pub fn set_dedup_open_paths(&mut self, enabled: bool) -> Result<(), MonitorError> {
        #[cfg(target_os = "linux")]
        return self.inner.set_dedup_open_paths(enabled);

        #[cfg(not(target_os = "linux"))]
        {
            let _ = enabled;
            Ok(())
        }
    }

    pub fn get_config_u32(&mut self, key: u32) -> Result<u32, MonitorError> {
        #[cfg(target_os = "linux")]
        return self.inner.get_config_u32(key);

        #[cfg(not(target_os = "linux"))]
        {
            let _ = key;
            Ok(0)
        }
    }

    /// Attach probes/tracepoints.
    pub fn attach(&mut self) -> Result<(), MonitorError> {
        #[cfg(target_os = "linux")]
        return self.inner.attach();

        #[cfg(not(target_os = "linux"))]
        Err(MonitorError::NotSupported)
    }

    /// Start reading events from the RingBuf and return a stream.
    pub fn listen(&mut self) -> Result<EventStream, MonitorError> {
        #[cfg(target_os = "linux")]
        return self.inner.listen();

        #[cfg(not(target_os = "linux"))]
        Err(MonitorError::NotSupported)
    }

    pub fn snapshot_stats(&mut self) -> Result<MonitorStatsSnapshot, MonitorError> {
        #[cfg(target_os = "linux")]
        return self.inner.snapshot_stats();

        #[cfg(not(target_os = "linux"))]
        Err(MonitorError::NotSupported)
    }
}

#[cfg(test)]
mod tests {
    use super::{validate_network_enforcement_support, MonitorStatsSnapshot};
    use assay_policy::tiers::{compile, FilePolicy, NetworkPolicy, Policy, ProcessPolicy};

    fn compiled_network_policy(
        allow_cidrs: &[&str],
        deny_cidrs: &[&str],
    ) -> assay_policy::tiers::CompiledPolicy {
        compile(&Policy {
            files: FilePolicy::default(),
            network: NetworkPolicy {
                allow_cidrs: allow_cidrs.iter().map(|cidr| (*cidr).to_string()).collect(),
                deny_cidrs: deny_cidrs.iter().map(|cidr| (*cidr).to_string()).collect(),
                ..Default::default()
            },
            processes: ProcessPolicy::default(),
        })
    }

    #[test]
    fn monitor_stats_snapshot_reports_ringbuf_pressure() {
        let stats = MonitorStatsSnapshot {
            tracepoint_ringbuf_dropped: 2,
            lsm_ringbuf_dropped: 1,
            socket_ringbuf_dropped: 3,
            ..Default::default()
        };

        assert!(stats.has_ringbuf_pressure());
        assert_eq!(stats.total_ringbuf_dropped(), 6);
    }

    #[test]
    fn ipv4_only_network_rules_are_supported() {
        let compiled = compiled_network_policy(&["10.0.0.0/8"], &["203.0.113.0/24"]);

        validate_network_enforcement_support(&compiled)
            .expect("the current target supports IPv4 CIDR rules");
    }

    #[test]
    fn ipv6_allow_rules_fail_closed_with_a_stable_reason() {
        let compiled = compiled_network_policy(&["2001:db8::/32"], &[]);

        let err = validate_network_enforcement_support(&compiled)
            .expect_err("an IPv6 allow rule must not be silently dropped");

        assert_eq!(
            err.to_string(),
            "network enforcement unavailable: the current enforcement target supports IPv4/TCP \
             only; policy contains 1 IPv6 allow CIDR rule(s) and 0 IPv6 deny CIDR rule(s)"
        );
    }

    #[test]
    fn ipv6_deny_rules_fail_closed_with_a_stable_reason() {
        let compiled = compiled_network_policy(&[], &["2001:db8:1::/48"]);

        let err = validate_network_enforcement_support(&compiled)
            .expect_err("an IPv6 deny rule must not be silently dropped");

        assert_eq!(
            err.to_string(),
            "network enforcement unavailable: the current enforcement target supports IPv4/TCP \
             only; policy contains 0 IPv6 allow CIDR rule(s) and 1 IPv6 deny CIDR rule(s)"
        );
    }

    #[test]
    fn mixed_ipv4_and_ipv6_rules_do_not_partially_apply() {
        let compiled = compiled_network_policy(
            &["10.0.0.0/8", "2001:db8::/32"],
            &["203.0.113.0/24", "2001:db8:1::/48"],
        );

        let err = validate_network_enforcement_support(&compiled)
            .expect_err("a mixed policy must be refused before its IPv4 subset can be applied");

        assert!(
            err.to_string()
                .contains("1 IPv6 allow CIDR rule(s) and 1 IPv6 deny CIDR rule(s)"),
            "the refusal must account for both unsupported rule classes: {err}"
        );
    }
}
