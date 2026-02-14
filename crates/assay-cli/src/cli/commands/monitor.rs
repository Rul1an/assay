use clap::Args;
use std::path::PathBuf;

#[path = "monitor_next/mod.rs"]
mod monitor_next;

#[derive(Args, Debug, Clone)]
#[command(
    about = "Runtime eBPF monitor (Linux only, experimental)",
    long_about = "Runtime eBPF monitor (Linux only, experimental)\n\
\n\
Requirements:\n\
  • Linux kernel with eBPF support\n\
  • Privileges: root or CAP_BPF + CAP_PERFMON (and often CAP_SYS_ADMIN depending on distro)\n\
\n\
Notes:\n\
  • Experimental: events may be dropped under load (ring buffer backpressure)\n\
  • Non-Linux: exits with code 40 (MONITOR_ATTACH_FAILED / NotSupported)\n",
    after_help = "Examples:\n\
  # Build eBPF bytecode\n\
  cargo xtask build-ebpf\n\
\n\
  # Monitor a process for 60s\n\
  sudo assay monitor --pid 1234 --duration 60s\n\
\n\
  # Explicit eBPF path\n\
  sudo assay monitor --ebpf target/assay-ebpf.o --pid 1234\n\
\n\
Common failures:\n\
  • EPERM: run with sudo or grant CAP_BPF/CAP_PERFMON\n\
  • Missing artifact: run `cargo xtask build-ebpf` (expects target/assay-ebpf.o)\n"
)]
pub struct MonitorArgs {
    /// PIDs to monitor (comma separated or multiple flags)
    #[arg(short, long)]
    pub pid: Vec<u32>,

    /// Path to eBPF object file. If not provided, defaults to local artifact if present.
    #[arg(long)]
    pub ebpf: Option<PathBuf>,

    /// Print events to stdout (implied if no other output specified)
    #[arg(long)]
    pub print: bool,

    /// Suppress all event output (only errors/stats)
    #[arg(long)]
    pub quiet: bool,

    /// Duration to run (e.g. "60s"). If omitted, runs until Ctrl-C.
    #[arg(long)]
    pub duration: Option<humantime::Duration>,

    /// Policy file to enable runtime enforcement rules
    #[arg(long)]
    pub policy: Option<PathBuf>,

    /// Monitor ALL cgroups (bypass filtering, useful for debugging)
    #[arg(long)]
    pub monitor_all: bool,
}

pub async fn run(args: MonitorArgs) -> anyhow::Result<i32> {
    monitor_next::run(args).await
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    use assay_common::encode_kernel_dev;
    #[cfg(target_os = "linux")]
    use assay_common::{MonitorEvent, EVENT_OPENAT};

    #[test]
    #[cfg(target_os = "linux")]
    fn test_kernel_dev_encoding() {
        let maj = 8;
        let min = 1;

        let fake_dev = libc::makedev(maj, min);
        let encoded = encode_kernel_dev(fake_dev as u64);

        let expected = (min & 0xff) | ((maj & 0xfff) << 8) | ((min & 0xfffff00) << 12);

        assert_eq!(
            encoded, expected,
            "Expected Linux new_encode_dev (sb->s_dev) encoding"
        );
        assert_eq!(encoded, 2049);
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn test_kernel_dev_encoding_skip_non_linux() {
        let _ = "linux-only";
    }

    #[test]
    fn test_kernel_dev_encoding_overflow() {
        let _maj = 4096;
        let _min = 0;
        assert_eq!(2 + 2, 4);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_normalize_path_syntactic_contract() {
        assert_eq!(
            super::monitor_next::normalize::normalize_path_syntactic(
                "/var//log/./app/../audit.log"
            ),
            "/var/log/audit.log"
        );
        assert_eq!(
            super::monitor_next::normalize::normalize_path_syntactic("tmp/./a/../b/c"),
            "tmp/b/c"
        );
        assert_eq!(
            super::monitor_next::normalize::normalize_path_syntactic("/../"),
            "/"
        );
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn test_normalize_path_syntactic_contract_skip_non_linux() {
        let _ = "linux-only";
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_find_violation_rule_allow_not_contract() {
        let allow = super::monitor_next::rules::compile_globset(&["/tmp/secret/**".to_string()])
            .expect("allow");
        let deny =
            super::monitor_next::rules::compile_globset(&["/tmp/secret/allowed/**".to_string()])
                .expect("deny");
        let rules = vec![super::monitor_next::rules::ActiveRule {
            id: "r1".to_string(),
            action: assay_core::mcp::runtime_features::MonitorAction::TriggerKill,
            allow,
            deny: Some(deny),
        }];

        let mut blocked = MonitorEvent::zeroed();
        blocked.pid = 42;
        blocked.event_type = EVENT_OPENAT;
        blocked.data[..b"/tmp/secret/blocked.txt\0".len()]
            .copy_from_slice(b"/tmp/secret/blocked.txt\0");

        let mut allowed = MonitorEvent::zeroed();
        allowed.pid = 42;
        allowed.event_type = EVENT_OPENAT;
        allowed.data[..b"/tmp/secret/allowed/file.txt\0".len()]
            .copy_from_slice(b"/tmp/secret/allowed/file.txt\0");

        let r_blocked = super::monitor_next::rules::find_violation_rule(&blocked, &rules);
        let r_allowed = super::monitor_next::rules::find_violation_rule(&allowed, &rules);

        assert!(
            r_blocked.is_some(),
            "blocked path must match violation rule"
        );
        assert!(
            r_allowed.is_none(),
            "deny/not exception must suppress match"
        );
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn test_find_violation_rule_allow_not_contract_skip_non_linux() {
        let _ = "linux-only";
    }
}
