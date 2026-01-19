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
    pub dev: u32,
    pub pad: u32,
    pub ino: u64,
    pub gen: u32,
    pub _pad2: u32,
}

/// Helper to encode userspace dev_t into Linux Kernel internal `s_dev` format.
/// Matches `new_encode_dev` in `include/linux/kdev_t.h`.
/// Format: (minor & 0xff) | (major << 8) | ((minor & !0xff) << 12)
pub fn encode_kernel_dev(dev: u64) -> u32 {
    let major = libc::major(dev as libc::dev_t) as u32;
    let minor = libc::minor(dev as libc::dev_t) as u32;
    (minor & 0xff) | (major << 8) | ((minor & !0xff) << 12)
}

#[cfg(target_os = "linux")]
unsafe impl aya::Pod for InodeKey {}

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
