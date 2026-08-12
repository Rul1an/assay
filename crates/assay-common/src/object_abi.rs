//! Object ABI digest; private descriptor is the sole `KEY_*` definition site.

/// ELF symbol baked by `assay-ebpf`, resolved by `assay-monitor` before `Ebpf::load`.
pub const OBJECT_ABI_SYMBOL: &str = "ASSAY_MONITOR_OBJECT_ABI";

macro_rules! define_monitor_config_keys {
    ($( $(#[$m:meta])* $name:ident = $id:expr ),* $(,)?) => {
        $( $(#[$m])* pub const $name: u32 = $id; )*
        #[rustfmt::skip]
        const MONITOR_CONFIG_KEYS: &[(&str, u32)] = &[$((stringify!($name), $name),)*];
    };
}

define_monitor_config_keys! {
    KEY_OFFSET_FILENAME = 0,
    KEY_OFFSET_SOCKADDR = 1,
    KEY_OFFSET_FORK_PARENT = 2,
    KEY_OFFSET_FORK_CHILD = 3,
    KEY_OFFSET_FILENAME_OPENAT2 = 4,
    KEY_OFFSET_OPENAT_FLAGS = 5,
    KEY_OFFSET_OPENAT_MODE = 6,
    KEY_OFFSET_OPENAT2_HOW = 7,
    KEY_OFFSET_SYSCALL_EXIT_RET = 8,
    KEY_MAX_ANCESTOR_DEPTH = 10,
    KEY_OFFSET_SENDTO_SOCKADDR = 11,
    KEY_OFFSET_SENDMSG_MSGHDR = 12,
    KEY_MONITOR_ALL = 100,
    KEY_EMIT_INODE_RESOLVED = 101,
    /// Emit an observed-connect event for every ALLOWED connect, not just blocked ones.
    ///
    /// Off by default and set only when a run asks for a peer set, because the allow path is the hot
    /// one: a monitored workload makes far more permitted connections than denied ones, and an
    /// unconditional emit would charge every existing user ring-buffer bandwidth for evidence they did
    /// not ask for. When it is off, `observed_peers` is honestly empty rather than quietly partial.
    KEY_EMIT_OBSERVED_CONNECT = 102,
    KEY_DEDUP_OPEN_PATHS = 103,
}

const fn mix(mut h: u32, b: u8) -> u32 {
    h ^= b as u32;
    h.wrapping_mul(0x0100_0193)
}

/// FNV-1a over name bytes + LE id.
pub const fn monitor_object_abi_digest() -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    let mut i = 0;
    while i < MONITOR_CONFIG_KEYS.len() {
        let (name, id) = MONITOR_CONFIG_KEYS[i];
        let bytes = name.as_bytes();
        let mut j = 0;
        while j < bytes.len() {
            hash = mix(hash, bytes[j]);
            j += 1;
        }
        hash = mix(hash, (id & 0xff) as u8);
        hash = mix(hash, ((id >> 8) & 0xff) as u8);
        hash = mix(hash, ((id >> 16) & 0xff) as u8);
        hash = mix(hash, ((id >> 24) & 0xff) as u8);
        i += 1;
    }
    hash
}

#[cfg(test)]
#[rustfmt::skip]
mod tests {
    use super::*;

    #[test]
    fn exported_keys_and_descriptor_derive_together() {
        assert_eq!(MONITOR_CONFIG_KEYS, &[
            ("KEY_OFFSET_FILENAME", 0),
            ("KEY_OFFSET_SOCKADDR", 1),
            ("KEY_OFFSET_FORK_PARENT", 2),
            ("KEY_OFFSET_FORK_CHILD", 3),
            ("KEY_OFFSET_FILENAME_OPENAT2", 4),
            ("KEY_OFFSET_OPENAT_FLAGS", 5),
            ("KEY_OFFSET_OPENAT_MODE", 6),
            ("KEY_OFFSET_OPENAT2_HOW", 7),
            ("KEY_OFFSET_SYSCALL_EXIT_RET", 8),
            ("KEY_MAX_ANCESTOR_DEPTH", 10),
            ("KEY_OFFSET_SENDTO_SOCKADDR", 11),
            ("KEY_OFFSET_SENDMSG_MSGHDR", 12),
            ("KEY_MONITOR_ALL", 100),
            ("KEY_EMIT_INODE_RESOLVED", 101),
            ("KEY_EMIT_OBSERVED_CONNECT", 102),
            ("KEY_DEDUP_OPEN_PATHS", 103),
        ]);
        assert_eq!(
            [
                KEY_OFFSET_FILENAME, KEY_OFFSET_SOCKADDR, KEY_OFFSET_FORK_PARENT,
                KEY_OFFSET_FORK_CHILD, KEY_OFFSET_FILENAME_OPENAT2, KEY_OFFSET_OPENAT_FLAGS,
                KEY_OFFSET_OPENAT_MODE, KEY_OFFSET_OPENAT2_HOW, KEY_OFFSET_SYSCALL_EXIT_RET,
                KEY_MAX_ANCESTOR_DEPTH, KEY_OFFSET_SENDTO_SOCKADDR, KEY_OFFSET_SENDMSG_MSGHDR,
                KEY_MONITOR_ALL, KEY_EMIT_INODE_RESOLVED, KEY_EMIT_OBSERVED_CONNECT,
                KEY_DEDUP_OPEN_PATHS,
            ],
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 100, 101, 102, 103],
        );
        // Independent golden over the full CONFIG identity table (names + LE ids).
        assert_eq!(monitor_object_abi_digest(), 0x4db6_b07a);
    }
}
