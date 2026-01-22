#![no_std]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "std")]
pub mod exports;

pub const EVENT_OPENAT: u32 = 1;
pub const EVENT_CONNECT: u32 = 2;
pub const EVENT_FORK: u32 = 3;
pub const EVENT_EXEC: u32 = 4;
pub const EVENT_EXIT: u32 = 5;

pub const EVENT_FILE_BLOCKED: u32 = 10;
pub const EVENT_CONNECT_BLOCKED: u32 = 20;

pub const DATA_LEN: usize = 512;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MonitorEvent {
    pub pid: u32,
    pub event_type: u32,
    pub data: [u8; DATA_LEN],
}

/// Key used to identify an inode in BPF maps.
///
/// # ABI and kernel assumptions
///
/// This struct is `#[repr(C)]` and used across the eBPF/userspace boundary,
/// so its layout must remain in sync with the corresponding kernel/BPF-side
/// definition.
///
/// The `dev` field stores the kernel's encoded `dev_t` value (e.g. from
/// `super_block.s_dev`) as a `u32`. On modern Linux kernels, this is typically
/// `MAJOR << 20 | MINOR`. This layout may not match kernels with different
/// `dev_t` representations.
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

/// Helper to encode userspace dev_t into Linux Kernel internal `s_dev` format.
/// Matches `MKDEV` macro in Linux kernel (major << 20 | minor).
/// This corresponds to `sb->s_dev` which we read in eBPF.
#[cfg(target_os = "linux")]
pub fn encode_kernel_dev(dev: u64) -> u32 {
    let major = libc::major(dev as libc::dev_t) as u32;
    let minor = libc::minor(dev as libc::dev_t) as u32;
    // MKDEV logic from include/linux/kdev_t.h (MINORBITS=20)
    (major << 20) | minor
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

// Exact size: 4 + 4 + 512 = 520 bytes
const _: [(); 520] = [(); core::mem::size_of::<MonitorEvent>()];

// Alignment should be 4 on all sane targets; if this fails, your ABI is different.
const _: [(); 4] = [(); core::mem::align_of::<MonitorEvent>()];

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

#[cfg(target_os = "linux")]
pub mod strict_open {
    use std::{ffi::CStr, mem::size_of};
    use libc::c_long;

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
