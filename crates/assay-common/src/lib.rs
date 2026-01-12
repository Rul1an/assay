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

pub const DATA_LEN: usize = 256;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MonitorEvent {
    pub pid: u32,
    pub event_type: u32,
    pub data: [u8; DATA_LEN],
}

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

// Exact size: 4 + 4 + 256 = 264 bytes
const _: [(); 264] = [(); core::mem::size_of::<MonitorEvent>()];

// Alignment should be 4 on all sane targets; if this fails, your ABI is different.
const _: [(); 4] = [(); core::mem::align_of::<MonitorEvent>()];
