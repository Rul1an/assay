//! Wave3 Step2 monitor split implementation behind stable facade.
//!
//! Contract:
//! - `monitor.rs` remains the public facade.
//! - This module hosts implementation details and preserves behavior.

#[cfg(target_os = "linux")]
use crate::exit_codes;
#[cfg(target_os = "linux")]
use assay_common::encode_kernel_dev;
#[cfg(target_os = "linux")]
use futures::FutureExt;
#[cfg(target_os = "linux")]
use tokio_stream::StreamExt;

#[cfg(target_os = "linux")]
pub(crate) mod errors;
#[cfg(target_os = "linux")]
pub(crate) mod events;
#[cfg(target_os = "linux")]
pub(crate) mod normalize;
// Cross-platform carrier; its only emitter (run_linux) is Linux-gated, so on other targets the
// constructors are unused outside the unit tests.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) mod enforcement_health;
// Same split as enforcement_health above, and for the same reason: the carrier and its rules are
// plain data, so they stay testable on a developer machine, while only the emitter that fills them
// from a live run is Linux-and-runner gated. Gating the whole module meant these tests never ran
// off Linux, which is how a coverage claim keyed on the wrong probe survived to a live run.
#[cfg_attr(not(all(target_os = "linux", feature = "runner")), allow(dead_code))]
// Gated to match its own dependency and its only callers. `observation_health` imports
// `assay_runner_schema`, which is an optional dep behind the `runner` feature, and both call sites
// below already carry `#[cfg(feature = "runner")]`. The module declaration did not, so any build
// without `runner` -- which is what the Wave 0 feature matrix builds -- failed on the import while
// the default build passed.
#[cfg(feature = "runner")]
pub(crate) mod observation_health;
// Same split as enforcement_health above: the record is cross-platform so its invariants stay
// testable on a developer machine, while the producer that fills it from connect events is
// Linux-and-runner gated, leaving `new`/`write_to` unused elsewhere.
#[cfg_attr(not(all(target_os = "linux", feature = "runner")), allow(dead_code))]
pub(crate) mod observed_peers;
pub(crate) mod output;
#[cfg(target_os = "linux")]
pub(crate) mod rules;
#[cfg(target_os = "linux")]
pub(crate) mod syscall_linux;
pub(crate) mod tests;

#[cfg(all(feature = "runner", any(target_os = "linux", test)))]
struct ObservationArtifacts {
    observed_peers: observed_peers::ObservedPeers,
    observation_health: assay_runner_schema::ObservationHealth,
}

#[cfg(all(feature = "runner", any(target_os = "linux", test)))]
fn build_observation_artifacts(
    run_id: &str,
    attachment: &assay_monitor::probes::ProbeAttachment,
    ringbuf_drops: u64,
    policy_declared: bool,
    peers: Vec<String>,
) -> ObservationArtifacts {
    let observed_peers = observed_peers::ObservedPeers::new(run_id, peers);
    let observation_health =
        observation_health::build(run_id, attachment, ringbuf_drops, policy_declared);

    ObservationArtifacts {
        observed_peers,
        observation_health,
    }
}

#[cfg(all(feature = "runner", any(target_os = "linux", test)))]
fn resolved_observation_target(path: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    if path.exists() {
        return std::fs::canonicalize(path);
    }

    let file_name = path.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "observation artifact output path must name a file",
        )
    })?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    Ok(std::fs::canonicalize(parent)?.join(file_name))
}

#[cfg(all(feature = "runner", any(target_os = "linux", test)))]
fn prepare_observation_artifact_targets(
    observed_peers: Option<&std::path::Path>,
    observation_health: Option<&std::path::Path>,
) -> std::io::Result<()> {
    if let (Some(peers), Some(health)) = (observed_peers, observation_health) {
        if resolved_observation_target(peers)? == resolved_observation_target(health)? {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "observed_peers and observation_health must use different output paths",
            ));
        }
    }

    for path in [observed_peers, observation_health].into_iter().flatten() {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
macro_rules! emit_out {
    ($($arg:tt)*) => {
        output::out(format!($($arg)*))
    };
}

macro_rules! emit_err {
    ($($arg:tt)*) => {
        output::err(format!($($arg)*))
    };
}

pub(crate) async fn run(args: super::MonitorArgs) -> anyhow::Result<i32> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = args;
        emit_err!("Error: 'assay monitor' is only supported on Linux.");
        Ok(40)
    }

    #[cfg(target_os = "linux")]
    {
        run_linux(args).await
    }
}

#[cfg(any(target_os = "linux", test))]
fn enforcement_failure_exit(health_written: bool, retained_exit: i32) -> i32 {
    if health_written {
        retained_exit
    } else {
        crate::exit_codes::EXIT_INFRA_ERROR
    }
}

#[cfg(any(target_os = "linux", test))]
fn tier1_enforcement_requested(compiled: &assay_policy::tiers::CompiledPolicy) -> bool {
    let tier1 = &compiled.tier1;
    !tier1.file_deny_exact.is_empty()
        || !tier1.file_deny_prefix.is_empty()
        || !tier1.inode_deny_exact.is_empty()
        || !tier1.network_allow_cidrs.is_empty()
        || !tier1.network_deny_cidrs.is_empty()
        || !tier1.network_deny_ports.is_empty()
        || !tier1.network_allow_ports.is_empty()
}

#[cfg(any(target_os = "linux", test))]
fn network_enforcement_requested(compiled: &assay_policy::tiers::CompiledPolicy) -> bool {
    !compiled.tier1.network_deny_ports.is_empty() || !compiled.tier1.network_deny_cidrs.is_empty()
}

#[cfg(any(target_os = "linux", test))]
fn startup_failure_health(
    network_enforcement_requested: bool,
) -> enforcement_health::EnforcementHealth {
    if network_enforcement_requested {
        enforcement_health::EnforcementHealth::failed(enforcement_health::SCOPE_IPV4_TCP_CONNECT)
    } else {
        enforcement_health::EnforcementHealth::absent(enforcement_health::SCOPE_IPV4_TCP_CONNECT)
    }
}

#[cfg(target_os = "linux")]
fn compile_runtime_enforcement_policy(
    cfg: &assay_core::mcp::runtime_features::RuntimeMonitorConfig,
) -> assay_policy::tiers::CompiledPolicy {
    let mut policy = assay_policy::tiers::Policy::default();

    for rule in &cfg.rules {
        let is_enforcement = matches!(
            rule.action,
            assay_core::mcp::runtime_features::MonitorAction::TriggerKill
                | assay_core::mcp::runtime_features::MonitorAction::Deny
        );

        if !is_enforcement {
            continue;
        }

        match rule.rule_type {
            assay_core::mcp::runtime_features::MonitorRuleType::FileOpen => {
                policy
                    .files
                    .deny
                    .extend(rule.match_config.path_globs.iter().cloned());
                if let Some(not) = &rule.match_config.not {
                    policy.files.allow.extend(not.path_globs.iter().cloned());
                }
            }
            assay_core::mcp::runtime_features::MonitorRuleType::NetConnect => {
                for dest in &rule.match_config.dest_globs {
                    let is_cidr = if let Some((ip_part, prefix_part)) = dest.split_once('/') {
                        ip_part.parse::<std::net::IpAddr>().is_ok()
                            && prefix_part.parse::<u8>().is_ok()
                    } else {
                        false
                    };

                    if is_cidr {
                        policy.network.deny_cidrs.push(dest.clone());
                    } else if let Ok(port) = dest.parse::<u16>() {
                        policy.network.deny_ports.push(port);
                    } else {
                        policy.network.deny_destinations.push(dest.clone());
                    }
                }
            }
            _ => {}
        }
    }

    assay_policy::tiers::compile(&policy)
}

#[cfg(target_os = "linux")]
async fn run_linux(args: super::MonitorArgs) -> anyhow::Result<i32> {
    use assay_common::{get_inode_generation, strict_open};
    use assay_monitor::Monitor;
    use enforcement_health::{EnforcementHealth, SCOPE_IPV4_TCP_CONNECT};

    #[cfg(feature = "runner")]
    let observation_run_id = if args.observation_health.is_some() || args.observed_peers.is_some() {
        if let Err(err) = prepare_observation_artifact_targets(
            args.observed_peers.as_deref(),
            args.observation_health.as_deref(),
        ) {
            emit_err!(
                "FATAL: observation artifact targets could not be prepared: {err}; refusing to emit a mixed old/new identity set"
            );
            return Ok(exit_codes::EXIT_INFRA_ERROR);
        }
        Some(observation_health::new_run_id())
    } else {
        None
    };

    let mut runtime_config = None;
    let mut kill_config = None;
    if let Some(path) = &args.policy {
        let p = assay_core::mcp::policy::McpPolicy::from_file(path)?;
        if let Some(rm) = p.runtime_monitor {
            if !rm.enabled {
                if !args.quiet {
                    emit_err!("Runtime monitor disabled by policy.");
                }
                return Ok(0);
            }
            runtime_config = Some(rm);
        }
        kill_config = p.kill_switch;
    }

    let compiled_policy = runtime_config
        .as_ref()
        .map(compile_runtime_enforcement_policy);
    let network_enforcement_requested = compiled_policy
        .as_ref()
        .map(network_enforcement_requested)
        .unwrap_or(false);

    if let Some(compiled) = compiled_policy.as_ref() {
        // The compiler accepts IPv6 CIDRs, but the shipped enforcement target attaches only
        // connect4 and loads only CIDR_RULES_V4. Refuse before loading or attaching eBPF; warning
        // and continuing would turn an IPv6 policy into an undisclosed IPv4 subset.
        if let Err(e) = assay_monitor::validate_network_enforcement_support(compiled) {
            emit_err!(
                "FATAL: egress enforcement policy cannot be installed: {} (fail-closed, not \
                 running a partially enforced policy)",
                e
            );
            return Ok(failed_enforcement_exit(&args, exit_codes::EXIT_WOULD_BLOCK));
        }
    }

    let ebpf_path = match args.ebpf.as_ref() {
        Some(p) => p.clone(),
        None => std::path::PathBuf::from("target/assay-ebpf.o"),
    };

    if !ebpf_path.exists() {
        emit_err!("Error: eBPF object not found at {}. Build it with 'cargo xtask build-ebpf' or provide --ebpf <path>", ebpf_path.display());
        return Ok(startup_failure_exit(
            &args,
            network_enforcement_requested,
            40,
        ));
    }

    let mut monitor = match Monitor::load_file(&ebpf_path) {
        Ok(m) => m,
        Err(e) => {
            emit_err!("Failed to load eBPF: {}", e);
            return Ok(startup_failure_exit(
                &args,
                network_enforcement_requested,
                40,
            ));
        }
    };

    if args.monitor_all {
        if !args.quiet {
            emit_out!("⚠️  MONITOR_ALL enabled: Bypassing Cgroup filtering.");
        }
        if let Err(e) = monitor.set_monitor_all(true) {
            emit_err!("Failed to enable MONITOR_ALL: {}", e);
            return Ok(startup_failure_exit(
                &args,
                network_enforcement_requested,
                40,
            ));
        }

        let v = match monitor.get_config_u32(assay_common::KEY_MONITOR_ALL) {
            Ok(value) => value,
            Err(e) => {
                emit_err!("Failed to verify MONITOR_ALL: {}", e);
                return Ok(startup_failure_exit(
                    &args,
                    network_enforcement_requested,
                    40,
                ));
            }
        };
        emit_out!(
            "DEBUG: CONFIG[{}]={} confirmed",
            assay_common::KEY_MONITOR_ALL,
            v
        );
        if v != 1 {
            emit_err!(
                "❌ Failed to enable MONITOR_ALL (CONFIG[{}] != 1)",
                assay_common::KEY_MONITOR_ALL
            );
            return Ok(startup_failure_exit(
                &args,
                network_enforcement_requested,
                40,
            ));
        }
    }

    if !args.pid.is_empty() {
        if let Err(e) = monitor.set_monitored_pids(&args.pid) {
            emit_err!("Warning: Failed to populate PID map: {}", e);
        }

        let mut cgroups = Vec::new();
        for &pid in &args.pid {
            match normalize::resolve_cgroup_id(pid) {
                Ok(id) => cgroups.push(id),
                Err(e) => emit_err!("Warning: Failed to resolve cgroup for PID {}: {}", pid, e),
            }
        }

        if !cgroups.is_empty() {
            if let Err(e) = monitor.set_monitored_cgroups(&cgroups) {
                emit_err!("Error: Failed to populate Cgroup map: {}", e);
                return Ok(startup_failure_exit(
                    &args,
                    network_enforcement_requested,
                    40,
                ));
            }
            if !args.quiet {
                emit_err!("Monitored Cgroups: {:?}", cgroups);
            }
        } else {
            emit_err!("Warning: No valid cgroups resolved. Rules will not match.");
        }
    }

    if let Err(e) = monitor.attach() {
        emit_err!("Failed to attach probes: {}", e);
        return Ok(startup_failure_exit(
            &args,
            network_enforcement_requested,
            40,
        ));
    }

    if !args.quiet {
        emit_err!("Assay Monitor running. Press Ctrl-C to stop.");
        if !args.pid.is_empty() {
            emit_err!("Monitoring PIDs: {:?}", args.pid);
        }
    }

    let rules = rules::compile_active_rules(runtime_config.as_ref());

    // Enforcement truth for the enforcement_health.v0 artifact: did egress enforcement actually attach?
    let mut enforcement_active = false;

    if let Some(mut compiled) = compiled_policy {
        let mut inode_rules = Vec::with_capacity(compiled.tier1.file_deny_exact.len());

        for rule in &compiled.tier1.file_deny_exact {
            use std::ffi::CString;
            use std::os::unix::ffi::OsStrExt;

            let c_path = match CString::new(std::path::Path::new(&rule.path).as_os_str().as_bytes())
            {
                Ok(c) => c,
                Err(e) => {
                    emit_err!("Warning: Invalid path encoding {} ({})", rule.path, e);
                    continue;
                }
            };

            let guard_fd_res = strict_open::openat2_strict(&c_path);
            let guard_fd = match guard_fd_res {
                Ok(fd) => fd,
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::Unsupported
                        || e.raw_os_error() == Some(libc::ENOSYS)
                    {
                        emit_err!(
                            "Warning: Strict open (openat2) unavailable on this system, using O_PATH fallback for {}",
                            rule.path
                        );
                        match syscall_linux::open_path_no_symlink(&c_path) {
                            Ok(fd) => fd,
                            Err(err) => {
                                emit_err!(
                                    "Warning: Fallback open failed for {}: {}",
                                    rule.path,
                                    err
                                );
                                continue;
                            }
                        }
                    } else if e.raw_os_error() == Some(libc::ELOOP)
                        || e.raw_os_error() == Some(libc::EXDEV)
                    {
                        emit_err!("Warning: Strict open blocked access to {} (Symlink/Breakout detected): {}", rule.path, e);
                        continue;
                    } else {
                        emit_err!("Warning: Failed to open denied path {}: {}", rule.path, e);
                        continue;
                    }
                }
            };

            let stat = match syscall_linux::fstat_fd(guard_fd) {
                Ok(stat) => stat,
                Err(e) => {
                    syscall_linux::close_fd(guard_fd);
                    emit_err!(
                        "Warning: Could not fstat denied path {} (skipping): {}",
                        rule.path,
                        e
                    );
                    continue;
                }
            };

            let gen = match get_inode_generation(guard_fd) {
                Ok(g) => g,
                Err(e) => {
                    let eno = e.raw_os_error().unwrap_or(0);
                    if eno == libc::ENOTTY || eno == libc::EINVAL {
                        0
                    } else {
                        emit_err!(
                            "Warning: Could not get inode generation for {} (using gen=0): {}",
                            rule.path,
                            e
                        );
                        0
                    }
                }
            };

            syscall_linux::close_fd(guard_fd);

            let dev = stat.st_dev;
            let ino = stat.st_ino;
            let kernel_dev = encode_kernel_dev(dev);

            if !args.quiet {
                let maj = libc::major(stat.st_dev);
                let min = libc::minor(stat.st_dev);
                emit_err!(
                    "Matched Inode for {}: dev={} (maj={}, min={}) -> kernel_dev={} ino={} gen={}",
                    rule.path,
                    dev,
                    maj,
                    min,
                    kernel_dev,
                    ino,
                    gen
                );
            }

            inode_rules.push(assay_policy::tiers::InodeRule {
                rule_id: rule.rule_id,
                dev: kernel_dev,
                ino,
                gen,
            });
        }

        compiled.tier1.inode_deny_exact.extend(inode_rules);

        if !args.quiet {
            emit_err!("Locked & Loaded Assurance Policy 🛡️");
            emit_err!("  • Tier 1 (Kernel): {} rules", compiled.stats.tier1_rules);
            emit_err!("  • Tier 2 (User):   {} rules", compiled.stats.tier2_rules);
            if !compiled.stats.warnings.is_empty() {
                for w in &compiled.stats.warnings {
                    emit_err!("    ⚠️  {}", w);
                }
            }
        }

        if let Err(e) = monitor.set_tier1_rules(&compiled) {
            if let Some(exit_code) =
                tier1_install_failure_exit(&args, &compiled, exit_codes::EXIT_WOULD_BLOCK)
            {
                emit_err!(
                    "FATAL: failed to install requested Tier 1 enforcement rules: {} \
                     (fail-closed, not reporting a partially loaded policy as active)",
                    e
                );
                return Ok(exit_code);
            }
            emit_err!(
                "Warning: Failed to load Tier 1 rules (LSM might be unavailable): {}",
                e
            );
        }

        // Egress enforcement (IPv4/TCP connect only): attach the connect4 cgroup program so the
        // compiled network deny rules actually block. Only attach when the policy requests it (has
        // network deny rules); the maps gate which connects are refused. Observation (the connect
        // tracepoint) is unchanged either way.
        //
        // FAIL-CLOSED: when enforcement is requested but cannot be installed (no cgroup v2 root, no
        // kernel support for cgroup/connect4, attach error), refuse to run rather than silently
        // continuing in audit-only mode. A consumer asking for egress enforcement must not get a clean
        // run that did not actually enforce.
        let has_net_deny = !compiled.tier1.network_deny_ports.is_empty()
            || !compiled.tier1.network_deny_cidrs.is_empty();
        if has_net_deny {
            let cgroup_file = match std::fs::File::open("/sys/fs/cgroup") {
                Ok(f) => f,
                Err(e) => {
                    emit_err!(
                        "FATAL: egress enforcement requested but cannot open cgroup v2 root /sys/fs/cgroup: {} (fail-closed, not running audit-only)",
                        e
                    );
                    // Honesty: a requested-but-failed enforcement must record `failed`, never look
                    // like `absent` (not requested), so write the artifact BEFORE the fail-closed exit.
                    // Distinguish enforcement refusal from an infrastructure failure to retain the
                    // requested carrier.
                    monitor.record_egress_failed("cgroup v2 root unavailable");
                    return Ok(failed_enforcement_exit(&args, exit_codes::EXIT_WOULD_BLOCK));
                }
            };
            if let Err(e) = monitor.attach_network_cgroup(&cgroup_file) {
                emit_err!(
                    "FATAL: egress enforcement requested but connect4 attach failed: {} (fail-closed, not running audit-only)",
                    e
                );
                return Ok(failed_enforcement_exit(&args, exit_codes::EXIT_WOULD_BLOCK));
            }
            // Fail-closed: a requested connect4 row must be terminal (Attached/Unavailable/Failed/
            // kernel-Unsupported), never left NotRequested and never reported as a clean run.
            if let Err(reason) = monitor.finalize_mode_aware(true) {
                emit_err!(
                    "FATAL: egress enforcement requested but mode-aware probe inventory is incomplete: {} (fail-closed)",
                    reason
                );
                return Ok(failed_enforcement_exit(&args, exit_codes::EXIT_WOULD_BLOCK));
            }
            enforcement_active = true;
            if !args.quiet {
                emit_err!(
                    "  • Egress enforcement ACTIVE (IPv4/TCP connect, connect4) at /sys/fs/cgroup: {} port + {} cidr deny rules. NOT covered: IPv6, UDP/QUIC, DNS resolution, already-open sockets, raw sockets, proxy/tunnel identity.",
                    compiled.tier1.network_deny_ports.len(),
                    compiled.tier1.network_deny_cidrs.len()
                );
            }
        }
    }

    // Ask the kernel for allowed-connect events only when this run wants a peer set. Off by
    // default, so a run that never asked pays nothing and reports an honestly empty set.
    if args.observed_peers.is_some() {
        if let Err(e) = monitor.set_emit_observed_connect(true) {
            emit_err!("FATAL: --observed-peers was requested but the observed-connect emit could not be enabled: {e}");
            return Ok(exit_codes::EXIT_INFRA_ERROR);
        }
    }

    let mut stream = match monitor.listen() {
        Ok(stream) => stream,
        Err(e) => {
            emit_err!("Failed to start monitor event stream: {}", e);
            return Ok(startup_failure_exit(
                &args,
                network_enforcement_requested,
                40,
            ));
        }
    };

    let mut observed_peers: Vec<String> = Vec::new();

    let mut timeout = match args.duration {
        Some(d) => tokio::time::sleep(d.into()).boxed(),
        None => std::future::pending().boxed(),
    };

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                if !args.quiet { emit_err!("\nStopping monitor..."); }
                break;
            }
            _ = &mut timeout => {
                if !args.quiet { emit_err!("\nDuration expired."); }
                break;
            }
            event_res = stream.next() => {
                match event_res {
                    Some(Ok(event)) => {
                        // Record the connect endpoints this run actually saw, so a later refutation
                        // reads an artifact rather than trusting an operator to retype them.
                        if let Some(peer) = events::observed_peer(&event) {
                            observed_peers.push(peer);
                        }
                        events::handle_event(&event, &args, &rules, kill_config.as_ref()).await;
                    }
                    Some(Err(e)) => {
                        emit_err!("Monitor stream error: {}", e);
                    }
                    None => {
                        emit_err!("Stream channel closed.");
                        break;
                    }
                }
            }
        }
    }

    // Enforcement block/allow counts for the enforcement_health artifact (from the kernel stats).
    let mut blocked_count = 0u64;
    let mut allowed_count = 0u64;
    let mut observed_ringbuf_drops = 0u64;
    match monitor.snapshot_stats() {
        Ok(stats) => {
            emit_err!("Monitor summary:");
            // Which probes actually attached. An unattached probe and a probe that attached and saw
            // nothing produce the same empty event stream, so the difference has to be stated: a
            // surface with no probe is unobserved, and silence on it is not evidence of absence.
            let attachment = monitor.probe_attachment();
            emit_err!(
                "  • Probes attached:    {}",
                if attachment.attached_probes().is_empty() {
                    "none".to_string()
                } else {
                    attachment.attached_probes().join(", ")
                }
            );
            if !attachment.is_complete() {
                emit_err!(
                    "  ⚠️  Unobserved surfaces (no probe attached): {}",
                    attachment.skipped_probes().join(", ")
                );
            }
            emit_err!(
                "  • Tracepoint ringbuf: emitted={} dropped={}",
                stats.tracepoint_events_emitted,
                stats.tracepoint_ringbuf_dropped
            );
            emit_err!("{}", output::format_send_observation_summary(&stats));
            emit_err!(
                "  • LSM ringbuf:        emitted={} dropped={}",
                stats.lsm_events_emitted,
                stats.lsm_ringbuf_dropped
            );
            emit_err!(
                "  • Socket policy:      checks={} blocked_cidr={} blocked_port={} allowed={} emitted={} dropped={}",
                stats.socket_checks,
                stats.socket_blocked_cidr,
                stats.socket_blocked_port,
                stats.socket_allowed,
                stats.socket_events_emitted,
                stats.socket_ringbuf_dropped
            );
            blocked_count = stats.socket_blocked_port + stats.socket_blocked_cidr;
            allowed_count = stats.socket_allowed;
            observed_ringbuf_drops = stats.total_ringbuf_dropped();
            if stats.has_ringbuf_pressure() {
                emit_err!(
                    "  ⚠️  Ring buffer pressure detected: {} dropped event(s)",
                    stats.total_ringbuf_dropped()
                );
            }
        }
        Err(e) => emit_err!("Warning: Failed to read monitor stats: {}", e),
    }

    // enforcement_health.v0 artifact (explicit, never parsed from stdout). active when enforcement
    // attached; absent when it was not requested. The `failed` case is written on the fail-closed abort
    // Network-enforcement validation, installation, and attach failures above write `failed`
    // before their handled nonzero exits.
    // observation_health.v0: what was watched, and what was not. Separate carrier from
    // enforcement_health by design — a run can observe completely and enforce nothing, or the
    // reverse. Coverage comes from probe ATTACHMENT, never from emitted counts; see the module.
    // Both artifacts are written here, from ONE run identity, on purpose. A refutation pairs a peer
    // set with a coverage descriptor, and that pairing is only meaningful within a single run: peers
    // from one run checked against another run's coverage would let a well-covered run vouch for a
    // blind one. Sharing `run_id` at the point of writing is what lets the consumer detect that.
    #[cfg(feature = "runner")]
    if let Some(run_id) = observation_run_id.as_deref() {
        let attachment = monitor.probe_attachment();
        let artifacts = build_observation_artifacts(
            run_id,
            &attachment,
            observed_ringbuf_drops,
            args.policy.is_some(),
            observed_peers,
        );

        if let Some(path) = args.observed_peers.as_ref() {
            if artifacts.observed_peers.write_to(path).is_err() {
                emit_err!(
                    "FATAL: observed_peers artifact was requested but could not be written; refusing exit 0 so a missing artifact is never read as an empty peer set"
                );
                return Ok(exit_codes::EXIT_INFRA_ERROR);
            }
        }

        if let Some(path) = args.observation_health.as_ref() {
            if !observation_health::write_to(&artifacts.observation_health, path) {
                // Same fail-closed rule as enforcement_health: a consumer reads a missing artifact as
                // "not requested", which would misreport a run that did observe.
                emit_err!(
                    "FATAL: observation_health artifact was requested but could not be written; refusing exit 0 so a missing artifact is never read as not-requested"
                );
                return Ok(exit_codes::EXIT_INFRA_ERROR);
            }
        }
    }

    if args.enforcement_health.is_some() {
        let health = if enforcement_active {
            EnforcementHealth::active(SCOPE_IPV4_TCP_CONNECT, blocked_count, allowed_count)
        } else {
            EnforcementHealth::absent(SCOPE_IPV4_TCP_CONNECT)
        };
        if !write_enforcement_health(&args, health) {
            // A requested artifact that cannot be written must not exit 0: a consumer reads a
            // missing file as "not requested" (absent), which would misreport an active run.
            // Enforcement itself worked, so this is an infra error, not a would-block.
            emit_err!(
                "FATAL: enforcement_health artifact was requested but could not be written; refusing exit 0 so a missing artifact is never read as not-requested"
            );
            return Ok(exit_codes::EXIT_INFRA_ERROR);
        }
    }

    Ok(exit_codes::OK)
}

/// Write the enforcement_health.v0 artifact to `--enforcement-health <path>` if set. No-op otherwise.
/// Returns `false` only when the artifact was requested but could not be written; on the fail-closed
/// abort paths the caller already exits non-zero, on the success path the caller must not exit 0.
#[cfg(any(target_os = "linux", test))]
fn write_enforcement_health(
    args: &super::MonitorArgs,
    health: enforcement_health::EnforcementHealth,
) -> bool {
    if let Some(path) = args.enforcement_health.as_ref() {
        match health.write_to(path) {
            Ok(()) => {
                output::err(format!(
                    "  • enforcement_health.v0 written: {} ({:?})",
                    path.display(),
                    health.network_enforcement
                ));
                true
            }
            Err(e) => {
                output::err(format!(
                    "ERROR: failed to write enforcement_health artifact to {}: {}",
                    path.display(),
                    e
                ));
                false
            }
        }
    } else {
        true
    }
}

#[cfg(target_os = "linux")]
fn failed_enforcement_exit(args: &super::MonitorArgs, retained_exit: i32) -> i32 {
    let health_written = write_enforcement_health(
        args,
        enforcement_health::EnforcementHealth::failed(enforcement_health::SCOPE_IPV4_TCP_CONNECT),
    );
    enforcement_failure_exit(health_written, retained_exit)
}

#[cfg(any(target_os = "linux", test))]
fn startup_failure_exit(
    args: &super::MonitorArgs,
    network_enforcement_requested: bool,
    retained_exit: i32,
) -> i32 {
    let health_written =
        write_enforcement_health(args, startup_failure_health(network_enforcement_requested));
    enforcement_failure_exit(health_written, retained_exit)
}

#[cfg(any(target_os = "linux", test))]
fn tier1_install_failure_exit(
    args: &super::MonitorArgs,
    compiled: &assay_policy::tiers::CompiledPolicy,
    retained_exit: i32,
) -> Option<i32> {
    tier1_enforcement_requested(compiled)
        .then(|| startup_failure_exit(args, network_enforcement_requested(compiled), retained_exit))
}
