use std::path::{Path, PathBuf};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::time::{SystemTime, UNIX_EPOCH};
use anyhow::{Context, Result, anyhow};

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
        let content = fs::read_to_string("/proc/self/cgroup")
            .context("Failed to read /proc/self/cgroup")?;

        let self_cgroup_line = content.lines()
            .find(|line| line.starts_with("0::"))
            .ok_or_else(|| anyhow!("Could not find Unified Hierarchy (0::) in /proc/self/cgroup"))?;

        let self_cgroup_path = self_cgroup_line.split("::").nth(1)
            .ok_or_else(|| anyhow!("Invalid cgroup line format"))?;

        // Handle root case "0::/" -> "" (empty relative path)
        let relative_path = if self_cgroup_path == "/" {
            Path::new("")
        } else {
            self_cgroup_path.strip_prefix('/').unwrap_or(self_cgroup_path).as_ref()
        };

        let root_path = mount_point.join(relative_path);

        if !root_path.exists() {
             return Err(anyhow!("Could not verify own cgroup path: {:?}", root_path));
        }

        Ok(Self { root_path })
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
        Ok(SessionCgroup { path, id: meta.ino() })
    }
}

impl SessionCgroup {
    pub fn id(&self) -> u64 { self.id }

    pub fn add_process(&self, pid: u32) -> Result<()> {
        let procs_path = self.path.join("cgroup.procs");
        fs::write(&procs_path, pid.to_string()).context("Failed to add PID")?;
        Ok(())
    }

    pub fn freeze(&self) -> Result<()> {
        let p = self.path.join("cgroup.freeze");
        if p.exists() { fs::write(p, "1")?; }
        Ok(())
    }

    pub fn thaw(&self) -> Result<()> {
        let p = self.path.join("cgroup.freeze");
        if p.exists() { fs::write(p, "0")?; }
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
        let mut pids: Vec<i32> = procs.lines()
            .filter_map(|l| l.trim().parse::<i32>().ok())
            .collect();
        // SOTA: Dedupe to prevent double-signaling (which is harmless but sloppy)
        pids.sort_unstable();
        pids.dedup();

        // Send SIGTERM via pidfd if possible
        for &pid in &pids {
            if pid <= 0 { continue; }

            #[cfg(target_os = "linux")]
            unsafe {
                // syscall(SYS_pidfd_open, pid, 0)
                let fd = libc::syscall(libc::SYS_pidfd_open, pid, 0) as i32;
                if fd >= 0 {
                    // syscall(SYS_pidfd_send_signal, fd, SIGTERM, NULL, 0)
                    let _ = libc::syscall(libc::SYS_pidfd_send_signal, fd, libc::SIGTERM, std::ptr::null::<libc::siginfo_t>(), 0);
                    libc::close(fd);
                } else {
                    // Fallback to classic kill if pidfd fails
                    libc::kill(pid, libc::SIGTERM);
                }
            }

            #[cfg(not(target_os = "linux"))]
            {
               use nix::sys::signal::{kill, Signal};
               use nix::unistd::Pid;
               let _ = kill(Pid::from_raw(pid), Signal::SIGTERM);
            }
        }

        let _ = self.thaw();
        std::thread::sleep(std::time::Duration::from_millis(grace_ms));
        self.kill()
    }

    pub fn set_pids_max(&self, max: u32) -> Result<()> {
        let p = self.path.join("pids.max");
        if p.exists() { fs::write(p, max.to_string())?; }
        Ok(())
    }

    /// P0 Hardening: Reliable cleanup
    pub fn remove(&self) -> Result<()> {
        if !self.path.exists() { return Ok(()); }

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
            eprintln!("Values to clean up cgroup session: {:?}", e);
        }
    }
}
