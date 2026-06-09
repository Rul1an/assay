//! Landlock-net CONNECT_TCP usability smoke (PR2).
//!
//! Proves the host can CREATE and APPLY a TCP-connect Landlock ruleset, one step beyond the PR1
//! ABI probe. This is host-eligibility / diagnostics only: it does NOT implement or claim
//! enforcement, blocks no real connection, and does not touch the sandbox path (`backend.rs`,
//! whose `// TODO(landlock-net)` stays untouched).
//!
//! Hybrid design (deliberate, per review):
//! - the parent builds the ruleset with the `landlock` crate (`AccessNet::ConnectTcp`, `NetPort`,
//!   `add_rule`) under `CompatLevel::HardRequirement` so CONNECT_TCP is never silently
//!   best-effort-dropped, then extracts the owned ruleset fd;
//! - a throwaway forked child runs ONLY async-signal-safe operations between fork and exit
//!   (`prctl(PR_SET_NO_NEW_PRIVS)`, the raw `landlock_restrict_self` syscall on the fd, `_exit`),
//!   so the diagnostics process itself is never restricted and no allocation/locking happens in
//!   the child. This mirrors the safe primitive a future enforcement child (PR5) must use.

use super::report::LandlockNetProbeStatus;

/// `LANDLOCK_ACCESS_NET_CONNECT_TCP` is an ABI >= 4 right; below that the smoke cannot run.
fn abi_supports_connect_tcp(abi_version: Option<u32>) -> bool {
    abi_version.is_some_and(|v| v >= 4)
}

/// Pure mapping from the throwaway child's wait result to a smoke status. Unit-tested
/// cross-platform. `exit_code` is `Some(code)` when the child exited normally (`code` is `0` on
/// success, otherwise the clamped failure errno written by the child), `None`/`signaled` mean the
/// child died abnormally.
///
/// On non-Linux the only non-test caller (`run_smoke`) is compiled out, so this is test-only there.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn classify_child(exit_code: Option<i32>, signaled: bool) -> (LandlockNetProbeStatus, Option<i32>) {
    if signaled {
        return (LandlockNetProbeStatus::Failed, None);
    }
    match exit_code {
        Some(0) => (LandlockNetProbeStatus::Usable, None),
        Some(code) => (LandlockNetProbeStatus::Failed, Some(code)),
        None => (LandlockNetProbeStatus::Failed, None),
    }
}

/// Run the CONNECT_TCP ruleset usability smoke. Returns the status plus the errno when it failed.
pub(super) fn probe_net_connect_ruleset(
    abi_version: Option<u32>,
) -> (LandlockNetProbeStatus, Option<i32>) {
    if !abi_supports_connect_tcp(abi_version) {
        return (LandlockNetProbeStatus::Unsupported, None);
    }
    #[cfg(target_os = "linux")]
    {
        run_smoke()
    }
    #[cfg(not(target_os = "linux"))]
    {
        // No Landlock off Linux; the ABI gate above already returns `Unsupported` in practice
        // (the syscall probe returns `None`), this is the belt-and-braces non-Linux arm.
        (LandlockNetProbeStatus::Unsupported, None)
    }
}

/// Walk an error's `source()` chain looking for an `io::Error`, and return its raw OS errno.
/// The `landlock` crate wraps the failing syscall's `io::Error` as a source on its build errors
/// (`CreateRulesetCall { source }`, `AddRuleCall { source }`), so this recovers the real errno for
/// a parent-side build failure.
#[cfg(target_os = "linux")]
fn errno_from_error(err: &(dyn std::error::Error + 'static)) -> Option<i32> {
    let mut cur: Option<&(dyn std::error::Error + 'static)> = Some(err);
    while let Some(e) = cur {
        if let Some(io) = e.downcast_ref::<std::io::Error>() {
            return io.raw_os_error();
        }
        cur = e.source();
    }
    None
}

/// A fixed, non-semantic test port. This smoke only validates that a CONNECT_TCP ruleset carrying
/// a port rule can be built and applied; it does NOT prove that connects to other ports are
/// blocked, so the specific port carries no claim.
#[cfg(target_os = "linux")]
const SMOKE_TEST_PORT: u16 = 443;

#[cfg(target_os = "linux")]
fn run_smoke() -> (LandlockNetProbeStatus, Option<i32>) {
    use landlock::{
        AccessNet, CompatLevel, Compatible, NetPort, Ruleset, RulesetAttr, RulesetCreatedAttr,
    };
    use std::os::fd::{AsRawFd, OwnedFd};

    // --- Parent: build the CONNECT_TCP ruleset (allocations are fine here). ---
    // HardRequirement => if CONNECT_TCP cannot be handled the build errors instead of silently
    // degrading to best-effort, which is the honest posture for an evidence-oriented smoke.
    let built = Ruleset::default()
        .set_compatibility(CompatLevel::HardRequirement)
        .handle_access(AccessNet::ConnectTcp)
        .and_then(|r| r.create())
        .and_then(|r| r.add_rule(NetPort::new(SMOKE_TEST_PORT, AccessNet::ConnectTcp)));

    let created = match built {
        Ok(c) => c,
        Err(e) => return (LandlockNetProbeStatus::Failed, errno_from_error(&e)),
    };

    // Extract the owned ruleset fd; the child applies it via a raw syscall.
    let owned_fd: Option<OwnedFd> = created.into();
    let owned_fd = match owned_fd {
        Some(fd) => fd,
        None => return (LandlockNetProbeStatus::Failed, Some(libc::EBADF)),
    };
    let ruleset_fd = owned_fd.as_raw_fd();

    // --- Throwaway child: only async-signal-safe calls between fork and exit. ---
    // SAFETY: the child runs only prctl, the raw landlock_restrict_self syscall, errno reads, and
    // `_exit` (all async-signal-safe); it performs no allocation and runs no Rust destructors.
    // The ruleset fd is inherited across fork. The parent never calls restrict_self, so the
    // diagnostics process itself is never restricted.
    const PR_SET_NO_NEW_PRIVS: libc::c_int = 38;
    let pid = unsafe { libc::fork() };
    if pid == 0 {
        // Child.
        if unsafe { libc::prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
            unsafe { libc::_exit(clamp_errno()) };
        }
        let ret = unsafe { libc::syscall(libc::SYS_landlock_restrict_self, ruleset_fd, 0) };
        if ret != 0 {
            unsafe { libc::_exit(clamp_errno()) };
        }
        unsafe { libc::_exit(0) };
    } else if pid < 0 {
        // fork failed in the parent.
        return (
            LandlockNetProbeStatus::Failed,
            std::io::Error::last_os_error().raw_os_error(),
        );
    }

    // --- Parent: reap the child and map its result. ---
    let mut status: libc::c_int = 0;
    let wait = unsafe { libc::waitpid(pid, &mut status, 0) };
    drop(owned_fd); // close the ruleset fd in the parent now that the child has run.
    if wait < 0 {
        return (LandlockNetProbeStatus::Failed, None);
    }
    if libc::WIFEXITED(status) {
        classify_child(Some(libc::WEXITSTATUS(status)), false)
    } else {
        // Signaled or otherwise abnormal.
        classify_child(None, true)
    }
}

/// Read `errno` and clamp it to a non-zero child exit code in `1..=255`. A failing syscall always
/// sets `errno`, but if it is unexpectedly `0` we return `255` so a failure never exits `0`
/// (which the parent reads as success). Async-signal-safe: a single `errno` read.
#[cfg(target_os = "linux")]
fn clamp_errno() -> libc::c_int {
    let e = unsafe { *libc::__errno_location() };
    if e <= 0 || e > 255 {
        255
    } else {
        e
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_gate_requires_v4() {
        assert!(!abi_supports_connect_tcp(None));
        assert!(!abi_supports_connect_tcp(Some(3)));
        assert!(abi_supports_connect_tcp(Some(4)));
        assert!(abi_supports_connect_tcp(Some(5)));
    }

    #[test]
    fn classify_child_success_is_usable() {
        assert_eq!(
            classify_child(Some(0), false),
            (LandlockNetProbeStatus::Usable, None)
        );
    }

    #[test]
    fn classify_child_nonzero_exit_is_failed_with_errno() {
        // EACCES and EPERM are the errnos a denied restrict_self / prctl would carry.
        assert_eq!(
            classify_child(Some(libc::EACCES), false),
            (LandlockNetProbeStatus::Failed, Some(libc::EACCES))
        );
        assert_eq!(
            classify_child(Some(libc::EPERM), false),
            (LandlockNetProbeStatus::Failed, Some(libc::EPERM))
        );
    }

    #[test]
    fn classify_child_signaled_is_failed_no_errno() {
        assert_eq!(
            classify_child(None, true),
            (LandlockNetProbeStatus::Failed, None)
        );
    }

    #[test]
    fn classify_child_no_exit_code_is_failed() {
        assert_eq!(
            classify_child(None, false),
            (LandlockNetProbeStatus::Failed, None)
        );
    }

    #[test]
    fn probe_is_unsupported_below_abi_4() {
        assert_eq!(
            probe_net_connect_ruleset(None),
            (LandlockNetProbeStatus::Unsupported, None)
        );
        assert_eq!(
            probe_net_connect_ruleset(Some(3)),
            (LandlockNetProbeStatus::Unsupported, None)
        );
    }

    #[test]
    fn probe_returns_without_panic_at_abi_4() {
        // Cross-platform we only assert it returns a value and never panics. On a non-Linux host
        // (or a Linux host without Landlock-net) this is `Unsupported`/`Failed`; on an ABI 4 host
        // it is `Usable`. The value itself is asserted in the VM evidence, not here.
        let (_status, _errno) = probe_net_connect_ruleset(Some(4));
    }
}
