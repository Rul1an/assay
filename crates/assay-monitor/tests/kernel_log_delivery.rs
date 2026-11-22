//! A real file-open denial must reach aya-log and release the logger's ring buffer on shutdown.
#![cfg(target_os = "linux")]

use assay_monitor::Monitor;
use assay_policy::tiers::{compile, InodeRule, Policy};
use std::{collections::BTreeSet, fs, os::unix::fs::MetadataExt, path::Path, time::Duration};
use tokio::sync::Notify;

struct Receiver(Notify);

impl log::Log for Receiver {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        if record.args().to_string()
            == format!("LSM Event {} Submitted", assay_common::EVENT_FILE_BLOCKED)
        {
            self.0.notify_one();
        }
    }

    fn flush(&self) {}
}

static RECEIVER: Receiver = Receiver(Notify::const_new());

fn ring_maps() -> BTreeSet<String> {
    let fdinfo = fs::canonicalize("/proc/self/fdinfo").unwrap();
    fs::read_dir(&fdinfo)
        .unwrap()
        .filter_map(|e| {
            let path = e.unwrap().path().canonicalize().ok()?;
            if !path.starts_with(&fdinfo) {
                return None;
            }
            let text = fs::read_to_string(path).ok()?;
            if !text
                .lines()
                .any(|l| l.split_whitespace().eq(["map_type:", "27"]))
            {
                return None;
            }
            Some(
                text.lines()
                    .find_map(|l| l.strip_prefix("map_id:"))
                    .unwrap()
                    .trim()
                    .to_owned(),
            )
        })
        .collect()
}

#[test]
#[ignore = "requires root, Linux BPF LSM, BTF and an ASSAY_EBPF_PATH object"]
fn delivers_kernel_log_and_releases_ring_on_monitor_drop() {
    // Monitor::attach is synchronous. A caller may have a timer but no I/O reactor.
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap()
        .block_on(check_delivery());
}

async fn check_delivery() {
    log::set_logger(&RECEIVER).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let victim = tempfile::NamedTempFile::new().unwrap();
    let stat = victim.as_file().metadata().unwrap();
    let baseline = ring_maps();
    let mut monitor = Monitor::load_file(std::env::var_os("ASSAY_EBPF_PATH").unwrap()).unwrap();
    let mut rules = compile(&Policy::default());
    rules.tier1.inode_deny_exact.push(InodeRule {
        dev: stat.dev().try_into().unwrap(),
        ino: stat.ino(),
        gen: 0,
        rule_id: 1,
    });
    monitor.set_tier1_rules(&rules).unwrap();
    let memberships = fs::read_to_string("/proc/self/cgroup").unwrap();
    let relative = memberships
        .lines()
        .find_map(|l| l.strip_prefix("0::"))
        .unwrap();
    let cg = Path::new("/sys/fs/cgroup").join(relative.trim_start_matches('/'));
    monitor
        .set_monitored_cgroups(&[fs::metadata(cg).unwrap().ino()])
        .unwrap();
    monitor.attach().expect("attach including file_open LSM");
    assert_ne!(ring_maps(), baseline);
    assert_eq!(
        fs::File::open(victim.path()).unwrap_err().raw_os_error(),
        Some(1)
    );
    tokio::time::timeout(Duration::from_secs(3), RECEIVER.0.notified())
        .await
        .expect("kernel denial log");
    drop(monitor);
    tokio::time::timeout(Duration::from_secs(3), async {
        while ring_maps() != baseline {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("release monitor ring maps");
    fs::File::open(victim.path()).expect("file accessible after detaching");
}
