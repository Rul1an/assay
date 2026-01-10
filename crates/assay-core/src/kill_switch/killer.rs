use super::{KillMode, KillReport, KillRequest};
#[allow(unused_imports)]
use anyhow::Context;

#[cfg(unix)]
use nix::sys::signal::{kill, Signal};
#[cfg(unix)]
use nix::unistd::Pid;

pub fn kill_pid(req: KillRequest) -> anyhow::Result<KillReport> {
    let mut incident_dir = None;

    // Optional: capture before kill
    if req.capture_state {
        if let Some(dir) = super::incident::write_incident_bundle_pre_kill(&req)? {
            incident_dir = Some(dir);
        }
    }

    // Kill main pid
    let success = match req.mode {
        KillMode::Immediate => kill_immediate(req.pid)?,
        KillMode::Graceful { grace } => kill_graceful(req.pid, grace)?,
    };

    // Kill children if requested
    let children_killed = if req.kill_children {
        kill_descendants(req.pid).unwrap_or_default()
    } else {
        vec![]
    };

    // Post-kill update incident bundle
    if let Some(ref dir) = incident_dir {
        super::incident::write_incident_bundle_post_kill(dir, &req, success, &children_killed)?;
    }

    Ok(KillReport {
        pid: req.pid,
        success,
        children_killed,
        incident_dir,
        error: if success { None } else { Some("failed to terminate process".into()) },
    })
}

#[cfg(unix)]
fn kill_immediate(pid: u32) -> anyhow::Result<bool> {
    kill(Pid::from_raw(pid as i32), Signal::SIGKILL)
        .with_context(|| format!("SIGKILL failed for pid={pid}"))?;
    Ok(true)
}

#[cfg(not(unix))]
fn kill_immediate(_pid: u32) -> anyhow::Result<bool> {
    anyhow::bail!("Kill Switch is not supported on this platform in v1.8 (Windows coming in v1.9)")
}

#[cfg(unix)]
fn kill_graceful(pid: u32, grace: std::time::Duration) -> anyhow::Result<bool> {
    let target = Pid::from_raw(pid as i32);

    // SIGTERM
    let _ = kill(target, Signal::SIGTERM);

    // wait a bit (polling)
    let start = std::time::Instant::now();
    while start.elapsed() < grace {
        if !is_running(pid) {
            return Ok(true);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    // SIGKILL
    let _ = kill(target, Signal::SIGKILL);
    Ok(!is_running(pid))
}

#[cfg(not(unix))]
fn kill_graceful(_pid: u32, _grace: std::time::Duration) -> anyhow::Result<bool> {
    anyhow::bail!("Kill Switch is not supported on this platform in v1.8 (Windows coming in v1.9)")
}

fn is_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // Portable enough: /proc exists on Linux; on macOS it doesn't.
        // For macOS we fall back to sysinfo if available.
        #[cfg(target_os = "linux")]
        {
            std::path::Path::new(&format!("/proc/{pid}")).exists()
        }
        #[cfg(not(target_os = "linux"))]
        {
            is_running_sysinfo(pid)
        }
    }
    #[cfg(not(unix))]
    {
        // Suppress unused warning for the argument when this branch is taken
        let _ = pid;
        // Logic for Windows is technically not supported yet, so this function returning false is fine/irrelevant
        // since kill_graceful (the only caller) bails out early anyway.
        false
    }
}

// Suppress dead code warning on Linux where this might not be called if /proc is used exclusively
#[allow(dead_code)]
fn is_running_sysinfo(pid: u32) -> bool {
    #[cfg(feature = "kill-switch")]
    {
        use sysinfo::{System, Pid as SPid};
        let mut sys = System::new();
        sys.refresh_processes();
        sys.process(SPid::from_u32(pid)).is_some()
    }
    #[cfg(not(feature = "kill-switch"))]
    {
        let _ = pid;
        false
    }
}

fn kill_descendants(parent_pid: u32) -> anyhow::Result<Vec<u32>> {
    #[cfg(feature = "kill-switch")]
    {
        use sysinfo::{System, Pid as SPid};

        let mut sys = System::new_all();
        sys.refresh_processes();

        #[allow(unused_mut)]
        let mut killed = vec![];
        let parent = SPid::from_u32(parent_pid);

        // Build descendants list
        let mut to_kill = vec![];
        for (pid, proc_) in sys.processes() {
            if let Some(ppid) = proc_.parent() {
                if ppid == parent {
                    to_kill.push(pid.as_u32());
                }
            }
        }

        // Naive BFS (good enough for v1.8 P0)
        let mut idx = 0;
        while idx < to_kill.len() {
            let cur = SPid::from_u32(to_kill[idx]);
            for (pid, proc_) in sys.processes() {
                if proc_.parent() == Some(cur) {
                    to_kill.push(pid.as_u32());
                }
            }
            idx += 1;
        }

        // Kill children first (reverse order helps)
        to_kill.sort_unstable();
        to_kill.dedup();
        to_kill.reverse();

        for pid in to_kill {
            #[cfg(unix)]
            {
                let _ = nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), nix::sys::signal::Signal::SIGKILL);
                killed.push(pid);
            }
            #[cfg(not(unix))]
            {
                let _ = pid; // Suppress unused variable on Windows
            }
        }

        Ok(killed)
    }
            #[cfg(unix)]
            {
                let _ = nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), nix::sys::signal::Signal::SIGKILL);
                killed.push(pid);
            }
        }

        Ok(killed)
    }
    #[cfg(not(feature = "kill-switch"))]
    {
        let _ = parent_pid;
        Ok(vec![])
    }
}
