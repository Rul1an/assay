#![no_std]
#![allow(unsafe_code)]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "std")]
pub mod exports;

/// Bounded ingest primitive (ADR-043 §1). std-only: it is an `io::Read` adapter.
pub mod limits;

/// DSSE Pre-Authentication Encoding. std-only: it allocates. Shared because two
/// implementations of PAE are two definitions of what a signature covers.
#[cfg(feature = "std")]
pub mod dsse;

/// MCP policy tool-name patterns. Shared for the same reason as `dsse`: a pattern language
/// answers "what does this policy permit", so two constructions of it are two answers, and a
/// policy file cannot show which one is being applied to it. Not std-gated — every operation is
/// a `&str` method and nothing allocates.
pub mod tool_pattern;

/// Monitor object ABI digest (`KEY_*` descriptor); shared by eBPF bake + monitor verify.
mod object_abi;
pub use object_abi::*;

pub const EVENT_OPENAT: u32 = 1;
pub const EVENT_CONNECT: u32 = 2;
pub const EVENT_FORK: u32 = 3;
pub const EVENT_EXEC: u32 = 4;
pub const EVENT_EXIT: u32 = 5;
pub const EVENT_SENDTO: u32 = 6;
pub const EVENT_SENDMSG: u32 = 7;

pub const EVENT_FILE_BLOCKED: u32 = 10;
pub const EVENT_CONNECT_BLOCKED: u32 = 20;
/// An ALLOWED connect, observed at the cgroup hook.
///
/// Carries the same projected payload as [`EVENT_CONNECT_BLOCKED`], so one decoder reads both, and
/// deliberately NOT the raw-sockaddr shape the `sys_enter_connect` tracepoint emits. The tracepoint
/// reads the address out of userspace memory before the kernel copies it, which a process can change
/// underneath; this hook reads `user_ip4`/`user_ip6` from the kernel's own `bpf_sock_addr`. For
/// observability either would do. For refuting a claim of the form "nothing went to X" only this one
/// is sound, because there the input is attacker-influenced.
pub const EVENT_CONNECT_OBSERVED: u32 = 21;

pub const DATA_LEN: usize = 512;

pub const MONITOR_STATS_LEN: u32 = 18;
pub const MONITOR_STAT_TRACEPOINT_EVENTS_EMITTED: u32 = 0;
pub const MONITOR_STAT_TRACEPOINT_RINGBUF_DROPPED: u32 = 1;
pub const MONITOR_STAT_LSM_EVENTS_EMITTED: u32 = 2;
pub const MONITOR_STAT_LSM_RINGBUF_DROPPED: u32 = 3;
pub const MONITOR_STAT_OPENAT_EVENTS_EMITTED: u32 = 4;
pub const MONITOR_STAT_OPENAT_RINGBUF_DROPPED: u32 = 5;
pub const MONITOR_STAT_OPENAT2_EVENTS_EMITTED: u32 = 6;
pub const MONITOR_STAT_OPENAT2_RINGBUF_DROPPED: u32 = 7;
pub const MONITOR_STAT_CONNECT_EVENTS_EMITTED: u32 = 8;
pub const MONITOR_STAT_CONNECT_RINGBUF_DROPPED: u32 = 9;
pub const MONITOR_STAT_SENDTO_EVENTS_EMITTED: u32 = 10;
pub const MONITOR_STAT_SENDTO_RINGBUF_DROPPED: u32 = 11;
pub const MONITOR_STAT_SENDMSG_EVENTS_EMITTED: u32 = 12;
pub const MONITOR_STAT_SENDMSG_RINGBUF_DROPPED: u32 = 13;
// sendto/sendmsg observed with no recoverable peer address (a sendto with a null
// sockaddr, or a sendmsg with no msg_name). Socket type is not classified here,
// so this includes address-less connected sends regardless of stream/datagram.
// Counted so these sends are visible rather than silently dropped.
pub const MONITOR_STAT_SENDTO_NO_PEER: u32 = 14;
pub const MONITOR_STAT_SENDMSG_NO_PEER: u32 = 15;
// sendto/sendmsg to a non-IP socket family (e.g. AF_UNIX) — observed but skipped
// because only IPv4/IPv6 peers are normalized into endpoint evidence. Socket type
// is not classified. Counted so the `datagram_peer_observed` label stays honest:
// it reflects IP peers only, and these sends are neither IP peers nor silently
// lost.
pub const MONITOR_STAT_SENDTO_NON_IP_FAMILY: u32 = 16;
pub const MONITOR_STAT_SENDMSG_NON_IP_FAMILY: u32 = 17;

pub const SOCKET_STATS_LEN: u32 = 8;
pub const SOCKET_STAT_CHECKS: u32 = 0;
pub const SOCKET_STAT_BLOCKED_CIDR: u32 = 1;
pub const SOCKET_STAT_BLOCKED_PORT: u32 = 2;
pub const SOCKET_STAT_ALLOWED: u32 = 3;
pub const SOCKET_STAT_EVENTS_EMITTED: u32 = 4;
pub const SOCKET_STAT_RINGBUF_DROPPED: u32 = 5;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MonitorEvent {
    pub pid: u32,
    pub event_type: u32,
    pub flags: u64,
    pub mode: u64,
    pub resolve: u64,
    pub return_value: i64,
    pub data: [u8; DATA_LEN],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SocketEvent {
    pub event_type: u32,
    pub pid: u32,
    pub timestamp_ns: u64,
    pub cgroup_id: u64,
    pub family: u16,
    pub port: u16,
    pub addr_v4: u32,
    pub addr_v6: [u8; 16],
    pub rule_id: u32,
    pub action: u32,
}

/// Tier-1 rule actions, as encoded by the policy compiler, branched on by the
/// `connect4` / `connect6` hooks, and reported in [`SocketEvent::action`].
pub const RULE_ACTION_ALLOW: u32 = 1;
pub const RULE_ACTION_DENY: u32 = 2;

/// Value stored in the `CIDR_RULES_V4` / `CIDR_RULES_V6` LPM tries.
///
/// # Why both fields
///
/// The tries hold allow *and* deny entries, so the kernel needs `action` to decide
/// whether a longest-prefix match blocks the connect. It separately needs `rule_id`
/// to report *which* policy rule matched: `SocketEvent::rule_id` is surfaced to
/// users by the monitor live view and recorded in runner evidence, so a value that
/// merely repeats the action would attribute every CIDR block to the same
/// nonexistent rule. Keeping both in one map value is what lets the hook answer
/// "blocked" and "by which rule" from a single lookup.
///
/// `#[repr(C)]` and used across the eBPF/userspace boundary, so the layout must
/// stay in sync on both sides; [`CIDR_RULE_VALUE_SIZE`] pins it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CidrRuleValue {
    /// [`RULE_ACTION_ALLOW`] or [`RULE_ACTION_DENY`].
    pub action: u32,
    /// Compiler-assigned id of the policy rule this entry came from. Ids start at
    /// 1, so 0 means "no rule" and never identifies a real rule.
    pub rule_id: u32,
}

/// Wire size of [`CidrRuleValue`]. Asserted against the real layout below so a
/// field added on one side of the boundary fails the build rather than shifting
/// how the other side reads the map.
pub const CIDR_RULE_VALUE_SIZE: usize = 8;

/// Key used to identify an inode in BPF maps.
///
/// # ABI and kernel assumptions
///
/// This struct is `#[repr(C)]` and used across the eBPF/userspace boundary,
/// so its layout must remain in sync with the corresponding kernel/BPF-side
/// definition.
///
/// The `dev` field stores the kernel's encoded `dev_t` value (e.g. from
/// `super_block.s_dev`) as a `u32`. On modern Linux kernels, this uses the
/// `new_encode_dev()` format:
///   (minor & 0xff) | ((major & 0xfff) << 8) | ((minor & 0xfffff00) << 12)
/// This matches `sb->s_dev` seen by eBPF.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct InodeKey {
    pub ino: u64,
    pub dev: u32,
    pub gen: u32,
}

/// Explicit 16-byte key for BPF Map Lookups (ino + dev + gen).
/// Layout: | ino (8) | dev (4) | gen (4) | = 16 bytes.
/// Guarantees dense packing without padding issues.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct InodeKeyMap {
    pub ino: u64,
    pub dev: u32,
    pub gen: u32,
}

/// Encode userspace dev_t into Linux kernel's `new_encode_dev()` format (sb->s_dev).
/// This matches include/linux/kdev_t.h:
///   new_encode_dev(MKDEV(major,minor)) for 32-bit dev_t encoding used in-kernel.
#[cfg(target_os = "linux")]
pub fn encode_kernel_dev(dev: u64) -> u32 {
    // Replicate Linux/glibc major()/minor() from <sys/sysmacros.h>
    // major(dev) = ((dev >> 8) & 0xfff)
    // minor(dev) = (dev & 0xff) | ((dev >> 12) & 0xfff00)
    let major = ((dev >> 8) & 0xfff) as u32;
    let minor = ((dev & 0xff) | ((dev >> 12) & 0xfff00)) as u32;

    // new_encode_dev:
    // (minor & 0xff) | ((major & 0xfff) << 8) | ((minor & 0xfffff00) << 12)
    (minor & 0xff) | ((major & 0xfff) << 8) | ((minor & 0xfffff00) << 12)
}

// Event ID for Inode Resolution telemetry
pub const EVENT_INODE_RESOLVED: u32 = 112;

// Shared ABI struct for Event 112
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct InodeResolved {
    pub dev: u64, // s_dev cast to u64
    pub ino: u64, // i_ino
    pub gen: u32, // i_generation
    pub _pad: u32,
}

#[cfg(all(target_os = "linux", feature = "user"))]
unsafe impl aya::Pod for InodeResolved {}

#[cfg(all(target_os = "linux", feature = "user"))]
unsafe impl aya::Pod for InodeKeyMap {}

#[cfg(all(target_os = "linux", feature = "user"))]
unsafe impl aya::Pod for InodeKey {}

#[cfg(all(target_os = "linux", feature = "user"))]
unsafe impl aya::Pod for CidrRuleValue {}

#[cfg(all(target_os = "linux", feature = "user"))]
const _: () = {
    fn _assert_pod<T: aya::Pod>() {}
    fn _check() {
        _assert_pod::<InodeKeyMap>();
    }
};

impl MonitorEvent {
    pub const fn zeroed() -> Self {
        Self {
            pid: 0,
            event_type: 0,
            flags: 0,
            mode: 0,
            resolve: 0,
            return_value: 0,
            data: [0u8; DATA_LEN],
        }
    }
}

impl Default for MonitorEvent {
    fn default() -> Self {
        Self::zeroed()
    }
}

// -----------------------------
// Compile-time ABI/layout checks
// -----------------------------

// Exact size: 4 + 4 + 8 + 8 + 8 + 8 + 512 = 552 bytes.
const _: [(); 552] = [(); core::mem::size_of::<MonitorEvent>()];

// Alignment is 8 because the ABI carries u64/i64 syscall metadata.
const _: [(); 8] = [(); core::mem::align_of::<MonitorEvent>()];

// SocketEvent is the eBPF/userspace ABI emitted by cgroup_sock_addr hooks.
// Size: 4 + 4 + 8 + 8 + 2 + 2 + 4 + 16 + 4 + 4 = 56 bytes.
const _: [(); 56] = [(); core::mem::size_of::<SocketEvent>()];
const _: [(); 8] = [(); core::mem::align_of::<SocketEvent>()];

// CidrRuleValue is the CIDR-trie map value shared by the hooks and the loader.
// Size: 4 + 4 = 8 bytes, no padding, so the kernel and userspace views agree.
const _: [(); CIDR_RULE_VALUE_SIZE] = [(); core::mem::size_of::<CidrRuleValue>()];
const _: [(); 4] = [(); core::mem::align_of::<CidrRuleValue>()];

#[cfg(all(target_os = "linux", feature = "std"))]
pub fn get_inode_generation(fd: std::os::fd::RawFd) -> std::io::Result<u32> {
    use nix::libc;
    use nix::request_code_read;

    // FS_IOC_GETVERSION is defined as _IOR('v', 1, long) in uapi/linux/fs.h
    // Kernel returns i_generation (32-bit value).
    // Best-effort: read into libc::c_long and cast to u32.

    // build ioctl request code: _IOR('v', 1, long)
    const fn fs_ioc_getversion() -> libc::c_ulong {
        request_code_read!(b'v', 1, core::mem::size_of::<libc::c_long>()) as libc::c_ulong
    }

    let mut out: libc::c_long = 0;
    // Safety: ioctl is unsafe, passing valid fd and pointer to initialized memory (0)
    // Cast request code to whatever the platform libc expects (c_ulong or c_int)
    let rc = unsafe { libc::ioctl(fd, fs_ioc_getversion() as _, &mut out) };
    if rc < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(out as u32)
}

#[cfg(all(target_os = "linux", feature = "std"))]
pub mod strict_open {
    use libc::c_long;
    use std::{ffi::CStr, mem::size_of};

    #[repr(C)]
    pub struct OpenHow {
        pub flags: u64,
        pub mode: u64,
        pub resolve: u64,
    }

    // UAPI openat2 resolve flags (uapi/linux/openat2.h)
    pub const RESOLVE_NO_SYMLINKS: u64 = 0x04;
    // pub const RESOLVE_BENEATH: u64 = 0x08; // Too strict for some CI setups (cross-device /tmp)

    pub fn openat2_strict(path: &CStr) -> std::io::Result<i32> {
        let how = OpenHow {
            flags: (libc::O_RDONLY | libc::O_NONBLOCK | libc::O_CLOEXEC) as u64,
            mode: 0,
            resolve: RESOLVE_NO_SYMLINKS, // Removed RESOLVE_BENEATH to avoid EXDEV in CI
        };

        let fd = unsafe {
            libc::syscall(
                libc::SYS_openat2,
                libc::AT_FDCWD,
                path.as_ptr(),
                &how as *const OpenHow,
                size_of::<OpenHow>(),
            ) as c_long
        };

        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(fd as i32)
    }
}
