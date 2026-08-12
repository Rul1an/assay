//! Object ABI digest from structured CONFIG `KEY_*`.
/// ELF symbol baked by `assay-ebpf`, resolved by `assay-monitor` before `Ebpf::load`.
pub const OBJECT_ABI_SYMBOL: &str = "ASSAY_MONITOR_OBJECT_ABI";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConfigKeyDesc {
    pub name: &'static str,
    pub id: u32,
}

#[rustfmt::skip]
pub const MONITOR_CONFIG_KEYS: &[ConfigKeyDesc] = &[
    ConfigKeyDesc { name: "KEY_MONITOR_ALL", id: crate::KEY_MONITOR_ALL },
    ConfigKeyDesc { name: "KEY_EMIT_INODE_RESOLVED", id: crate::KEY_EMIT_INODE_RESOLVED },
    ConfigKeyDesc { name: "KEY_EMIT_OBSERVED_CONNECT", id: crate::KEY_EMIT_OBSERVED_CONNECT },
    ConfigKeyDesc { name: "KEY_DEDUP_OPEN_PATHS", id: crate::KEY_DEDUP_OPEN_PATHS },
];

const fn mix(mut h: u32, b: u8) -> u32 {
    h ^= b as u32;
    h.wrapping_mul(0x0100_0193)
}

/// FNV-1a over name bytes + LE id.
pub const fn monitor_object_abi_digest() -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    let mut i = 0;
    while i < MONITOR_CONFIG_KEYS.len() {
        let key = &MONITOR_CONFIG_KEYS[i];
        let mut j = 0;
        let name = key.name.as_bytes();
        while j < name.len() {
            hash = mix(hash, name[j]);
            j += 1;
        }
        let id = key.id;
        hash = mix(hash, (id & 0xff) as u8);
        hash = mix(hash, ((id >> 8) & 0xff) as u8);
        hash = mix(hash, ((id >> 16) & 0xff) as u8);
        hash = mix(hash, ((id >> 24) & 0xff) as u8);
        i += 1;
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{KEY_DEDUP_OPEN_PATHS, KEY_MONITOR_ALL};

    #[test]
    fn config_keys_span_feature_ids() {
        assert_eq!(MONITOR_CONFIG_KEYS.len(), 4);
        assert_eq!(MONITOR_CONFIG_KEYS[0].id, KEY_MONITOR_ALL);
        assert_eq!(MONITOR_CONFIG_KEYS[3].id, KEY_DEDUP_OPEN_PATHS);
    }

    #[test]
    fn digest_stable_nonzero() {
        let d = monitor_object_abi_digest();
        assert_ne!(d, 0);
        assert_eq!(d, monitor_object_abi_digest());
    }
}
