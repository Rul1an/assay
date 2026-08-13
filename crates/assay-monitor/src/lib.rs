mod error;
pub use error::MonitorError;

#[cfg(any(test, target_os = "linux"))]
mod config_flags;

pub mod events;
#[cfg(any(test, target_os = "linux"))]
mod object_abi;
pub mod probes;
#[cfg(any(test, target_os = "linux"))]
mod program_set;
pub mod tree;

#[cfg(target_os = "linux")]
mod loader;
#[cfg(target_os = "linux")]
pub mod tracepoint;

use assay_common::MonitorEvent;

#[cfg(any(test, target_os = "linux"))]
fn probe_inventory_result(result: Result<(), &'static str>) -> Result<(), MonitorError> {
    result.map_err(MonitorError::ProbeInventory)
}

#[cfg(test)]
mod probe_inventory_result_tests {
    use super::*;

    #[test]
    fn incomplete_probe_inventory_propagates_as_monitor_error() {
        assert!(matches!(
            probe_inventory_result(Err("missing terminal status")),
            Err(MonitorError::ProbeInventory("missing terminal status"))
        ));
    }
}

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

#[cfg(any(test, target_os = "linux"))]
use assay_common::{
    MONITOR_STAT_CONNECT_EVENTS_EMITTED, MONITOR_STAT_CONNECT_RINGBUF_DROPPED,
    MONITOR_STAT_LSM_EVENTS_EMITTED, MONITOR_STAT_LSM_RINGBUF_DROPPED,
    MONITOR_STAT_OPENAT2_EVENTS_EMITTED, MONITOR_STAT_OPENAT2_RINGBUF_DROPPED,
    MONITOR_STAT_OPENAT_EVENTS_EMITTED, MONITOR_STAT_OPENAT_RINGBUF_DROPPED,
    MONITOR_STAT_SENDMSG_EVENTS_EMITTED, MONITOR_STAT_SENDMSG_NON_IP_FAMILY,
    MONITOR_STAT_SENDMSG_NO_PEER, MONITOR_STAT_SENDMSG_RINGBUF_DROPPED,
    MONITOR_STAT_SENDTO_EVENTS_EMITTED, MONITOR_STAT_SENDTO_NON_IP_FAMILY,
    MONITOR_STAT_SENDTO_NO_PEER, MONITOR_STAT_SENDTO_RINGBUF_DROPPED,
    MONITOR_STAT_TRACEPOINT_EVENTS_EMITTED, MONITOR_STAT_TRACEPOINT_RINGBUF_DROPPED,
    SOCKET_STAT_ALLOWED, SOCKET_STAT_BLOCKED_CIDR, SOCKET_STAT_BLOCKED_PORT, SOCKET_STAT_CHECKS,
    SOCKET_STAT_EVENTS_EMITTED, SOCKET_STAT_RINGBUF_DROPPED,
};

/// Project both kernel stat arrays onto the snapshot.
///
/// One function rather than a list of separately deletable statements: `snapshot_stats` has no
/// snapshot to return without calling this. The readers differ in return type because the arrays do
/// -- `STATS` holds `u32`, `SOCKET_STATS` holds `u64` -- so they cannot be passed in swapped. Every
/// field is named here, with no `..Default::default()`, so a dropped read is a compile error rather
/// than a zero indistinguishable from a clean run.
#[cfg(any(test, target_os = "linux"))]
fn project_snapshot(
    mut read_stats: impl FnMut(u32) -> u32,
    mut read_socket: impl FnMut(u32) -> u64,
    event_size_mismatch: u64,
) -> MonitorStatsSnapshot {
    MonitorStatsSnapshot {
        tracepoint_events_emitted: read_stats(MONITOR_STAT_TRACEPOINT_EVENTS_EMITTED),
        tracepoint_ringbuf_dropped: read_stats(MONITOR_STAT_TRACEPOINT_RINGBUF_DROPPED),
        lsm_events_emitted: read_stats(MONITOR_STAT_LSM_EVENTS_EMITTED),
        lsm_ringbuf_dropped: read_stats(MONITOR_STAT_LSM_RINGBUF_DROPPED),
        openat_events_emitted: read_stats(MONITOR_STAT_OPENAT_EVENTS_EMITTED),
        openat_ringbuf_dropped: read_stats(MONITOR_STAT_OPENAT_RINGBUF_DROPPED),
        openat2_events_emitted: read_stats(MONITOR_STAT_OPENAT2_EVENTS_EMITTED),
        openat2_ringbuf_dropped: read_stats(MONITOR_STAT_OPENAT2_RINGBUF_DROPPED),
        connect_events_emitted: read_stats(MONITOR_STAT_CONNECT_EVENTS_EMITTED),
        connect_ringbuf_dropped: read_stats(MONITOR_STAT_CONNECT_RINGBUF_DROPPED),
        sendto_events_emitted: read_stats(MONITOR_STAT_SENDTO_EVENTS_EMITTED),
        sendto_ringbuf_dropped: read_stats(MONITOR_STAT_SENDTO_RINGBUF_DROPPED),
        sendmsg_events_emitted: read_stats(MONITOR_STAT_SENDMSG_EVENTS_EMITTED),
        sendmsg_ringbuf_dropped: read_stats(MONITOR_STAT_SENDMSG_RINGBUF_DROPPED),
        sendto_no_peer: read_stats(MONITOR_STAT_SENDTO_NO_PEER),
        sendmsg_no_peer: read_stats(MONITOR_STAT_SENDMSG_NO_PEER),
        sendto_non_ip_family: read_stats(MONITOR_STAT_SENDTO_NON_IP_FAMILY),
        sendmsg_non_ip_family: read_stats(MONITOR_STAT_SENDMSG_NON_IP_FAMILY),
        socket_checks: read_socket(SOCKET_STAT_CHECKS),
        socket_blocked_cidr: read_socket(SOCKET_STAT_BLOCKED_CIDR),
        socket_blocked_port: read_socket(SOCKET_STAT_BLOCKED_PORT),
        socket_allowed: read_socket(SOCKET_STAT_ALLOWED),
        socket_events_emitted: read_socket(SOCKET_STAT_EVENTS_EMITTED),
        socket_ringbuf_dropped: read_socket(SOCKET_STAT_RINGBUF_DROPPED),
        event_size_mismatch,
    }
}

#[cfg(test)]
mod snapshot_projection_tests {
    use super::*;

    /// Totality: each reader returns its own key, so a correct projection produces the snapshot
    /// whose every field is the key it was read from. One comparison, so no field can be omitted
    /// from the check, and a field added to the struct is a compile error here.
    #[test]
    fn every_snapshot_field_reads_its_own_key() {
        assert_eq!(
            project_snapshot(|k| k, u64::from, 777),
            MonitorStatsSnapshot {
                tracepoint_events_emitted: MONITOR_STAT_TRACEPOINT_EVENTS_EMITTED,
                tracepoint_ringbuf_dropped: MONITOR_STAT_TRACEPOINT_RINGBUF_DROPPED,
                lsm_events_emitted: MONITOR_STAT_LSM_EVENTS_EMITTED,
                lsm_ringbuf_dropped: MONITOR_STAT_LSM_RINGBUF_DROPPED,
                openat_events_emitted: MONITOR_STAT_OPENAT_EVENTS_EMITTED,
                openat_ringbuf_dropped: MONITOR_STAT_OPENAT_RINGBUF_DROPPED,
                openat2_events_emitted: MONITOR_STAT_OPENAT2_EVENTS_EMITTED,
                openat2_ringbuf_dropped: MONITOR_STAT_OPENAT2_RINGBUF_DROPPED,
                connect_events_emitted: MONITOR_STAT_CONNECT_EVENTS_EMITTED,
                connect_ringbuf_dropped: MONITOR_STAT_CONNECT_RINGBUF_DROPPED,
                sendto_events_emitted: MONITOR_STAT_SENDTO_EVENTS_EMITTED,
                sendto_ringbuf_dropped: MONITOR_STAT_SENDTO_RINGBUF_DROPPED,
                sendmsg_events_emitted: MONITOR_STAT_SENDMSG_EVENTS_EMITTED,
                sendmsg_ringbuf_dropped: MONITOR_STAT_SENDMSG_RINGBUF_DROPPED,
                sendto_no_peer: MONITOR_STAT_SENDTO_NO_PEER,
                sendmsg_no_peer: MONITOR_STAT_SENDMSG_NO_PEER,
                sendto_non_ip_family: MONITOR_STAT_SENDTO_NON_IP_FAMILY,
                sendmsg_non_ip_family: MONITOR_STAT_SENDMSG_NON_IP_FAMILY,
                socket_checks: u64::from(SOCKET_STAT_CHECKS),
                socket_blocked_cidr: u64::from(SOCKET_STAT_BLOCKED_CIDR),
                socket_blocked_port: u64::from(SOCKET_STAT_BLOCKED_PORT),
                socket_allowed: u64::from(SOCKET_STAT_ALLOWED),
                socket_events_emitted: u64::from(SOCKET_STAT_EVENTS_EMITTED),
                socket_ringbuf_dropped: u64::from(SOCKET_STAT_RINGBUF_DROPPED),
                event_size_mismatch: 777,
            }
        );
    }

    /// A key that returns itself only discriminates between fields if the keys differ: two fields
    /// reading one key, or two constants sharing a value, would let them swap undetected. Recording
    /// what each reader was actually asked for checks that on the projection itself.
    #[test]
    fn no_two_fields_read_the_same_key() {
        let (mut stats_keys, mut socket_keys) = (Vec::new(), Vec::new());
        project_snapshot(
            |k| {
                stats_keys.push(k);
                k
            },
            |k| {
                socket_keys.push(k);
                u64::from(k)
            },
            0,
        );
        assert_eq!(
            (stats_keys.len(), socket_keys.len()),
            (18, 6),
            "reads per array"
        );
        for keys in [&mut stats_keys, &mut socket_keys] {
            let read = keys.len();
            keys.sort_unstable();
            keys.dedup();
            assert_eq!(keys.len(), read, "two fields read the same key");
        }
    }
}

/// Per-hook attribution of drops on the tracepoint ring, and the part nothing claims.
///
/// #1271 asks the diagnostic projection to distinguish loss layers, because "conflating them into
/// one ring-buffer drops number is precisely what makes future failures hard to triage". This is
/// that split for the tracepoint ring.
///
/// `unattributed` is not padding. Every hook bumps `MONITOR_STAT_TRACEPOINT_RINGBUF_DROPPED`, and
/// only some bump a per-hook counter as well -- `sched_process_fork` (`fork_events.rs:46`) does
/// not. A breakdown that reported only the five it can name would silently understate the ring,
/// and a reader comparing it against `ringbuf_drops` would find a gap with nothing to explain it.
/// Reporting the remainder is what makes the attribution safe to read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TracepointDropAttribution {
    pub openat: u32,
    pub openat2: u32,
    pub connect: u32,
    pub sendto: u32,
    pub sendmsg: u32,
    /// Drops on the tracepoint ring that no per-hook counter claims.
    pub unattributed: u64,
}

impl TracepointDropAttribution {
    /// The part of the ring total this breakdown can name.
    pub fn attributed(&self) -> u64 {
        u64::from(self.openat)
            + u64::from(self.openat2)
            + u64::from(self.connect)
            + u64::from(self.sendto)
            + u64::from(self.sendmsg)
    }

    /// Attributed plus unattributed. Equals `tracepoint_ringbuf_dropped` by construction.
    pub fn total(&self) -> u64 {
        self.attributed() + self.unattributed
    }
}

impl MonitorStatsSnapshot {
    /// Split this snapshot's tracepoint-ring drops by the hook that lost the record.
    ///
    /// `saturating_sub` rather than an assertion: the counters are read from a live kernel map in
    /// no particular order, so a per-hook counter can be observed after the ring counter it was
    /// bumped alongside. A transient negative remainder is a read artefact, not a violated
    /// invariant, and clamping it reports zero unattributed rather than a nonsense number.
    pub fn tracepoint_drop_attribution(&self) -> TracepointDropAttribution {
        let mut attribution = TracepointDropAttribution {
            openat: self.openat_ringbuf_dropped,
            openat2: self.openat2_ringbuf_dropped,
            connect: self.connect_ringbuf_dropped,
            sendto: self.sendto_ringbuf_dropped,
            sendmsg: self.sendmsg_ringbuf_dropped,
            unattributed: 0,
        };
        attribution.unattributed =
            u64::from(self.tracepoint_ringbuf_dropped).saturating_sub(attribution.attributed());
        attribution
    }

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
    /// Which probes attached this run, and which did not.
    ///
    /// The input a coverage descriptor needs. A surface with no attached probe is unobserved, and
    /// silence on it is not evidence of absence — an unattached probe and a probe that attached and
    /// saw nothing produce the same empty event stream.
    ///
    /// On non-Linux targets nothing attaches, so every expected probe is reported as a blind spot
    /// rather than the record being absent: "no observation" is a fact, not a missing field.
    #[must_use]
    pub fn probe_attachment(&self) -> probes::ProbeAttachment {
        #[cfg(target_os = "linux")]
        {
            self.inner.probe_attachment().clone()
        }
        #[cfg(not(target_os = "linux"))]
        {
            let mut attachment = probes::ProbeAttachment::default();
            attachment.reconcile(probes::EXPECTED_PROBES);
            attachment
        }
    }

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

    /// connect4 `Failed` when policy requested but attach never ran (e.g. cgroup root missing).
    pub fn record_egress_failed(&mut self, reason: &'static str) {
        #[cfg(target_os = "linux")]
        self.inner.record_egress_failed(reason);
        #[cfg(not(target_os = "linux"))]
        {
            let _ = reason;
        }
    }

    pub fn finalize_mode_aware(&self, network_policy_requested: bool) -> Result<(), &'static str> {
        #[cfg(target_os = "linux")]
        return self
            .inner
            .probe_attachment()
            .finalize_mode_aware(network_policy_requested);
        #[cfg(not(target_os = "linux"))]
        {
            let _ = network_policy_requested;
            Ok(())
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

    /// Ask the kernel to emit an event for every ALLOWED connect, not only blocked ones.
    ///
    /// Off unless a run wants a peer set. The allow path is the hot one, so this is opt-in rather
    /// than default: a run that does not ask pays nothing, and its peer set is honestly empty
    /// instead of quietly partial.
    ///
    /// On a non-Linux host this is accepted and does nothing, because there is no kernel to ask.
    //
    // The doc used to read "See [`loader::LinuxMonitor::set_emit_observed_connect`]", which
    // rustdoc rejects under `--deny warnings`: `mod loader` is private, so the link pointed at
    // something a consumer of this crate cannot open. Deferring public documentation to a private
    // item is not a broken link to be re-spelled -- it is a doc that says nothing to its reader.
    pub fn set_emit_observed_connect(&mut self, enabled: bool) -> Result<(), MonitorError> {
        #[cfg(target_os = "linux")]
        {
            self.inner.set_emit_observed_connect(enabled)
        }
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

#[cfg(test)]
mod tracepoint_attribution_tests {
    use super::*;

    fn snapshot() -> MonitorStatsSnapshot {
        MonitorStatsSnapshot::default()
    }

    /// The attribution and the remainder reconstruct the ring counter exactly. Without this the
    /// breakdown is a set of numbers with no stated relationship to the one the gate reads.
    #[test]
    fn attribution_and_remainder_reconstruct_the_ring_total() {
        let mut s = snapshot();
        s.tracepoint_ringbuf_dropped = 17;
        s.openat_ringbuf_dropped = 3;
        s.openat2_ringbuf_dropped = 1;
        s.connect_ringbuf_dropped = 4;
        s.sendto_ringbuf_dropped = 2;
        s.sendmsg_ringbuf_dropped = 5;

        let a = s.tracepoint_drop_attribution();
        assert_eq!(a.attributed(), 15);
        assert_eq!(a.unattributed, 2, "the fork hook has no per-hook counter");
        assert_eq!(a.total(), u64::from(s.tracepoint_ringbuf_dropped));
    }

    /// A drop on a hook with no per-hook counter is reported as unattributed, not as absent.
    ///
    /// `sched_process_fork` bumps the ring counter and nothing else (`fork_events.rs:46`), so a
    /// breakdown that named only the five hooks it can attribute would show all-zeros against a
    /// non-zero `ringbuf_drops` — a gap with nothing to explain it, which is exactly the triage
    /// problem #1271 exists to remove.
    #[test]
    fn a_drop_no_hook_claims_is_reported_rather_than_lost() {
        let mut s = snapshot();
        s.tracepoint_ringbuf_dropped = 6;
        let a = s.tracepoint_drop_attribution();
        assert_eq!(a.attributed(), 0);
        assert_eq!(a.unattributed, 6);
        assert_eq!(a.total(), 6);
    }

    /// The two hooks whose counters userspace never read.
    ///
    /// Before they were wired, a `sendto`/`sendmsg` drop landed in `unattributed` and looked like a
    /// fork drop. The attribution was not wrong about the total; it was wrong about the cause,
    /// which is the only thing a breakdown is for.
    #[test]
    fn sendto_and_sendmsg_drops_are_attributed() {
        let mut s = snapshot();
        s.tracepoint_ringbuf_dropped = 9;
        s.sendto_ringbuf_dropped = 4;
        s.sendmsg_ringbuf_dropped = 5;
        let a = s.tracepoint_drop_attribution();
        assert_eq!((a.sendto, a.sendmsg), (4, 5));
        assert_eq!(a.unattributed, 0, "both are now claimed by their own hook");
    }

    /// A per-hook counter read after the ring counter it was bumped alongside must not underflow.
    #[test]
    fn a_transient_over_read_clamps_rather_than_wrapping() {
        let mut s = snapshot();
        s.tracepoint_ringbuf_dropped = 1;
        s.openat_ringbuf_dropped = 5;
        let a = s.tracepoint_drop_attribution();
        assert_eq!(a.unattributed, 0);
        assert!(a.attributed() >= u64::from(s.tracepoint_ringbuf_dropped));
    }

    /// The attribution never changes what the gate reads. `ringbuf_drops` is the ring total, and a
    /// breakdown that could move it would be changing acceptance rather than explaining it.
    #[test]
    fn attribution_does_not_touch_the_acceptance_total() {
        let mut s = snapshot();
        s.tracepoint_ringbuf_dropped = 2;
        s.lsm_ringbuf_dropped = 3;
        s.socket_ringbuf_dropped = 4;
        s.openat_ringbuf_dropped = 1;
        s.sendto_ringbuf_dropped = 1;
        let before = s.total_ringbuf_dropped();
        let _ = s.tracepoint_drop_attribution();
        assert_eq!(s.total_ringbuf_dropped(), before);
        assert_eq!(
            before, 9,
            "tracepoint + lsm + socket, the three actual rings"
        );
    }
}
