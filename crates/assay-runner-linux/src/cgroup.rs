use anyhow::{anyhow, Context, Result};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Manages Cgroup V2 operations for Assay.
pub struct CgroupManager {
    root_path: PathBuf,
}

/// Represents an active ephemeral Cgroup session.
pub struct SessionCgroup {
    path: PathBuf,
    id: u64, // Inode number (cgroup ID)
}

impl CgroupManager {
    /// Checks if Cgroup V2 is available and resolves the correct nesting root.
    /// SOTA Hardening: Detects current cgroup scope to support systemd slices.
    pub fn new() -> Result<Self> {
        let mount_point = PathBuf::from("/sys/fs/cgroup");

        if !mount_point.exists() || !mount_point.is_dir() {
            return Err(anyhow!("Cgroup V2 mount not found"));
        }

        // P0 Fix: Robust parsing of /proc/self/cgroup
        // We look for the line starting with "0::" (Unified hierarchy)
        let content =
            fs::read_to_string("/proc/self/cgroup").context("Failed to read /proc/self/cgroup")?;

        let self_cgroup_line = content
            .lines()
            .find(|line| line.starts_with("0::"))
            .ok_or_else(|| {
                anyhow!("Could not find Unified Hierarchy (0::) in /proc/self/cgroup")
            })?;

        let self_cgroup_path = self_cgroup_line
            .split("::")
            .nth(1)
            .ok_or_else(|| anyhow!("Invalid cgroup line format"))?;

        // Handle root case "0::/" -> "" (empty relative path)
        let relative_path = if self_cgroup_path == "/" {
            Path::new("")
        } else {
            self_cgroup_path
                .strip_prefix('/')
                .unwrap_or(self_cgroup_path)
                .as_ref()
        };

        let root_path = mount_point.join(relative_path);

        if !root_path.exists() {
            return Err(anyhow!("Could not verify own cgroup path: {:?}", root_path));
        }

        let root_path = Self::nearest_domain_root(&mount_point, root_path)?;

        Ok(Self { root_path })
    }

    fn nearest_domain_root(mount_point: &Path, mut path: PathBuf) -> Result<PathBuf> {
        loop {
            let cgroup_type = fs::read_to_string(path.join("cgroup.type"))
                .with_context(|| format!("Failed to read cgroup type for {}", path.display()))?;
            if cgroup_type.trim() == "domain" && !Self::is_systemd_scope(&path) {
                return Ok(path);
            }

            if path == mount_point {
                return Err(anyhow!(
                    "Could not find domain cgroup root at or above {}",
                    path.display()
                ));
            }

            path = path
                .parent()
                .ok_or_else(|| anyhow!("Could not ascend from cgroup path {}", path.display()))?
                .to_path_buf();
        }
    }

    fn is_systemd_scope(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".scope"))
    }

    /// Creates a new ephemeral cgroup.
    pub fn create_session(&self) -> Result<SessionCgroup> {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
        let name = format!("assay-session-{}", timestamp);
        let path = self.root_path.join(&name);

        // P0: Best-effort remove if collision (unlikely with timestamp)
        if path.exists() {
            let _ = fs::remove_dir(&path);
        }

        fs::create_dir(&path).context("Failed to create cgroup session dir")?;

        // Best effort enable pids controller
        let subtree = self.root_path.join("cgroup.subtree_control");
        if subtree.exists() {
            // Ignoring errors as we might lack delegation
            let _ = fs::write(&subtree, "+pids");
        }

        let meta = fs::metadata(&path)?;
        #[cfg(unix)]
        let id = meta.ino();
        #[cfg(not(unix))]
        let id = {
            let _ = meta;
            0
        }; // Stub for non-Unix
        Ok(SessionCgroup { path, id })
    }
}

impl SessionCgroup {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn procs_path(&self) -> PathBuf {
        self.path.join("cgroup.procs")
    }

    pub fn add_process(&self, pid: u32) -> Result<()> {
        let procs_path = self.procs_path();
        fs::write(&procs_path, pid.to_string()).context("Failed to add PID")?;
        Ok(())
    }

    pub fn freeze(&self) -> Result<()> {
        let p = self.path.join("cgroup.freeze");
        if p.exists() {
            fs::write(p, "1")?;
        }
        Ok(())
    }

    pub fn thaw(&self) -> Result<()> {
        let p = self.path.join("cgroup.freeze");
        if p.exists() {
            fs::write(p, "0")?;
        }
        Ok(())
    }

    pub fn kill(&self) -> Result<()> {
        let p = self.path.join("cgroup.kill");
        if p.exists() {
            fs::write(p, "1")?;
        } else {
            return Err(anyhow!("cgroup.kill missing"));
        }
        Ok(())
    }

    /// P0 Hardening: Graceful kill using pidfd to avoid PID reuse races.
    pub fn kill_graceful(&self, grace_ms: u64) -> Result<()> {
        let _ = self.freeze();

        let procs = fs::read_to_string(self.path.join("cgroup.procs"))?;
        let mut pids: Vec<i32> = procs
            .lines()
            .filter_map(|l| l.trim().parse::<i32>().ok())
            .collect();
        // SOTA: Dedupe to prevent double-signaling (which is harmless but sloppy)
        pids.sort_unstable();
        pids.dedup();

        // Send SIGTERM via pidfd if possible
        for &pid in &pids {
            if pid <= 0 {
                continue;
            }

            #[cfg(target_os = "linux")]
            unsafe {
                // syscall(SYS_pidfd_open, pid, 0)
                let fd = libc::syscall(libc::SYS_pidfd_open, pid, 0) as i32;
                if fd >= 0 {
                    // syscall(SYS_pidfd_send_signal, fd, SIGTERM, NULL, 0)
                    let _ = libc::syscall(
                        libc::SYS_pidfd_send_signal,
                        fd,
                        libc::SIGTERM,
                        std::ptr::null::<libc::siginfo_t>(),
                        0,
                    );
                    libc::close(fd);
                } else {
                    // Fallback to classic kill if pidfd fails
                    libc::kill(pid, libc::SIGTERM);
                }
            }

            #[cfg(all(unix, not(target_os = "linux")))]
            {
                use nix::sys::signal::{kill, Signal};
                use nix::unistd::Pid;
                let _ = kill(Pid::from_raw(pid), Signal::SIGTERM);
            }
            #[cfg(not(unix))]
            {
                // Windows/Other: No-op or standard process kill if we had handle
                // Since this uses PIDs from cgroup.procs (Linux concept), this code is unreachable logic-wise
                // but must compile.
                let _ = pid;
            }
        }

        let _ = self.thaw();
        std::thread::sleep(std::time::Duration::from_millis(grace_ms));
        self.kill()
    }

    pub fn set_pids_max(&self, max: u32) -> Result<()> {
        let p = self.path.join("pids.max");
        if p.exists() {
            fs::write(p, max.to_string())?;
        }
        Ok(())
    }

    /// P0 Hardening: Reliable cleanup
    pub fn remove(&self) -> Result<()> {
        if !self.path.exists() {
            return Ok(());
        }

        // Retry loop for cgroup removal (busy/not empty)
        for _ in 0..3 {
            match fs::remove_dir(&self.path) {
                Ok(_) => return Ok(()),
                Err(_) => {
                    // Try to kill remainders if still exists
                    let _ = self.kill();
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }
        // Final attempt
        fs::remove_dir(&self.path).context("Failed to remove cgroup after retries")
    }
}

impl Drop for SessionCgroup {
    fn drop(&mut self) {
        if let Err(e) = self.remove() {
            eprintln!("Failed to clean up cgroup session: {:?}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CgroupManager;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_cgroup_tree(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("assay-cgroup-{name}-{stamp}"));
        fs::create_dir_all(&path).expect("create temp cgroup tree");
        path
    }

    #[test]
    fn nearest_domain_root_accepts_domain_cgroup() {
        let root = temp_cgroup_tree("domain");
        fs::write(root.join("cgroup.type"), "domain\n").expect("write cgroup type");

        let resolved =
            CgroupManager::nearest_domain_root(&root, root.clone()).expect("resolve domain root");

        assert_eq!(resolved, root);
        fs::remove_dir_all(resolved).expect("remove temp cgroup tree");
    }

    #[test]
    fn nearest_domain_root_ascends_from_domain_threaded_service() {
        let root = temp_cgroup_tree("domain-threaded");
        let system_slice = root.join("system.slice");
        let service = system_slice.join("actions.runner.example.service");
        fs::create_dir_all(&service).expect("create fake service cgroup");
        fs::write(root.join("cgroup.type"), "domain\n").expect("write root cgroup type");
        fs::write(system_slice.join("cgroup.type"), "domain\n")
            .expect("write system slice cgroup type");
        fs::write(service.join("cgroup.type"), "domain threaded\n")
            .expect("write service cgroup type");

        let resolved =
            CgroupManager::nearest_domain_root(&root, service).expect("resolve domain root");

        assert_eq!(resolved, system_slice);
        fs::remove_dir_all(root).expect("remove temp cgroup tree");
    }

    #[test]
    fn nearest_domain_root_ascends_from_systemd_session_scope() {
        let root = temp_cgroup_tree("session-scope");
        let user_slice = root.join("user.slice");
        let user_id_slice = user_slice.join("user-1000.slice");
        let session_scope = user_id_slice.join("session-263.scope");
        fs::create_dir_all(&session_scope).expect("create fake session cgroup");
        fs::write(root.join("cgroup.type"), "domain\n").expect("write root cgroup type");
        fs::write(user_slice.join("cgroup.type"), "domain\n")
            .expect("write user slice cgroup type");
        fs::write(user_id_slice.join("cgroup.type"), "domain\n")
            .expect("write user id slice cgroup type");
        fs::write(session_scope.join("cgroup.type"), "domain\n")
            .expect("write session scope cgroup type");

        let resolved =
            CgroupManager::nearest_domain_root(&root, session_scope).expect("resolve domain root");

        assert_eq!(resolved, user_id_slice);
        fs::remove_dir_all(root).expect("remove temp cgroup tree");
    }
}
