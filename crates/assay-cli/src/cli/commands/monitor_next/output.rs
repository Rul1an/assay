#[cfg(target_os = "linux")]
use crate::cli::commands::monitor::MonitorArgs;

#[cfg(target_os = "linux")]
pub(crate) fn out(message: impl AsRef<str>) {
    println!("{}", message.as_ref());
}

pub(crate) fn err(message: impl AsRef<str>) {
    eprintln!("{}", message.as_ref());
}

#[cfg(any(target_os = "linux", test))]
pub(crate) fn decode_utf8_cstr(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    String::from_utf8_lossy(&data[..end]).to_string()
}

#[cfg(any(target_os = "linux", test))]
pub(crate) fn dump_prefix_hex(data: &[u8], n: usize) -> String {
    data.iter()
        .take(n)
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join("")
}

#[cfg(any(target_os = "linux", test))]
pub(crate) fn decode_file_blocked_payload(data: &[u8]) -> Option<(u64, u64, u64, u32)> {
    if data.len() < 28 {
        return None;
    }

    let dev = u64::from_ne_bytes(data[0..8].try_into().ok()?);
    let ino = u64::from_ne_bytes(data[8..16].try_into().ok()?);
    let cgroup_id = u64::from_ne_bytes(data[16..24].try_into().ok()?);
    let rule_id = u32::from_ne_bytes(data[24..28].try_into().ok()?);

    Some((dev, ino, cgroup_id, rule_id))
}

#[cfg(any(target_os = "linux", test))]
pub(crate) fn decode_blocked_net_payload(data: &[u8]) -> Option<(u64, String, u16, u32)> {
    use std::net::{Ipv4Addr, Ipv6Addr};

    if data.len() < 40 {
        return None;
    }

    let cgroup_id = u64::from_ne_bytes(data[0..8].try_into().ok()?);
    let family = u16::from_ne_bytes(data[8..10].try_into().ok()?);
    let port = u16::from_ne_bytes(data[10..12].try_into().ok()?);
    let rule_id = u32::from_ne_bytes(data[32..36].try_into().ok()?);
    let dst = match family {
        2 => {
            let addr = Ipv4Addr::new(data[12], data[13], data[14], data[15]);
            addr.to_string()
        }
        10 => {
            let addr = Ipv6Addr::from(<[u8; 16]>::try_from(&data[16..32]).ok()?);
            addr.to_string()
        }
        _ => return None,
    };

    Some((cgroup_id, dst, port, rule_id))
}

#[cfg(target_os = "linux")]
pub(crate) fn log_violation(pid: u32, rule_id: &str, quiet: bool) {
    if !quiet {
        println!(
            "[PID {}] 🚨 VIOLATION: Rule '{}' matched file access",
            pid, rule_id
        );
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn log_kill(
    pid: u32,
    mode: &assay_core::mcp::runtime_features::KillMode,
    grace: u64,
    quiet: bool,
) {
    if !quiet {
        println!(
            "[PID {}] 💀 INIT KILL (mode={:?}, grace={}ms)",
            pid, mode, grace
        );
    }
}

/// Format a monitor event into its human-readable line, or `None` when the event type produces no
/// output. Pure and platform-independent so the line shapes can be pinned by a unit test;
/// `log_monitor_event` is the thin stdout wrapper used on the live (Linux) capture path. These
/// formats are a producer contract: Plimsoll's capture scraper parses these exact shapes.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn format_monitor_event(event_type: u32, pid: u32, data: &[u8]) -> Option<String> {
    use assay_common::{EVENT_CONNECT, EVENT_FILE_BLOCKED, EVENT_OPENAT};

    // The live path always passes a full fixed-size event payload, but the slice contract is also
    // exercised by unit tests, so read fixed offsets with checked access and a zero fallback rather
    // than indexing, which would panic on a short buffer.
    fn read_u64(data: &[u8], start: usize) -> u64 {
        data.get(start..start + 8)
            .and_then(|s| <[u8; 8]>::try_from(s).ok())
            .map(u64::from_ne_bytes)
            .unwrap_or(0)
    }
    fn read_u32(data: &[u8], start: usize) -> u32 {
        data.get(start..start + 4)
            .and_then(|s| <[u8; 4]>::try_from(s).ok())
            .map(u32::from_ne_bytes)
            .unwrap_or(0)
    }

    let line = match event_type {
        EVENT_OPENAT => format!("[PID {}] openat: {}", pid, decode_utf8_cstr(data)),
        EVENT_CONNECT => format!(
            "[PID {}] connect sockaddr[0..32]=0x{}",
            pid,
            dump_prefix_hex(data, 32)
        ),
        EVENT_FILE_BLOCKED => match decode_file_blocked_payload(data) {
            Some((dev, ino, cgroup_id, rule_id)) => format!(
                "[PID {}] 🛡️ BLOCKED FILE: dev={} ino={} cgroup={} rule_id={}",
                pid, dev, ino, cgroup_id, rule_id
            ),
            None => format!(
                "[PID {}] 🛡️ BLOCKED FILE: 0x{}",
                pid,
                dump_prefix_hex(data, 32)
            ),
        },
        11 => format!("[PID {}] 🟢 ALLOWED FILE: {}", pid, decode_utf8_cstr(data)),
        20 => match decode_blocked_net_payload(data) {
            Some((cgroup_id, dst, port, rule_id)) => format!(
                "[PID {}] 🛡️ BLOCKED NET: dst={} port={} cgroup={} rule_id={}",
                pid, dst, port, cgroup_id, rule_id
            ),
            None => format!(
                "[PID {}] 🛡️ BLOCKED NET : {}",
                pid,
                dump_prefix_hex(data, 20)
            ),
        },
        112 => {
            let dev = read_u64(data, 0);
            let ino = read_u64(data, 8);
            let gen = read_u32(data, 16);
            format!(
                "[PID {}] 🔒 INODE RESOLVED: dev={} (0x{:x}) ino={} gen={}",
                pid, dev, dev, ino, gen
            )
        }
        101..=104 => {
            let chunk_idx = event_type - 101;
            let start_offset = chunk_idx * 64;
            let dump = dump_prefix_hex(data, 64);
            format!(
                "[PID {}] 🔍 STRUCT DUMP Part {} (Offset {}-{}): {}",
                pid,
                chunk_idx + 1,
                start_offset,
                start_offset + 64,
                dump
            )
        }
        105 => {
            let path = decode_utf8_cstr(data);
            format!("[PID {}] 📂 FILE OPEN (Manual Resolution): {}", pid, path)
        }
        106 => format!("[PID {}] 🐛 DEBUG: Dentry Pointer NULL", pid),
        107 => format!("[PID {}] 🐛 DEBUG: Name Pointer NULL", pid),
        108 => format!(
            "[PID {}] 🐛 DEBUG: LSM Hook Entry (MonitorAll={})",
            pid,
            data.first().copied().unwrap_or(0)
        ),
        109 => format!("[PID {}] 🐛 DEBUG: Passed Monitor Check", pid),
        110 => {
            let ptr = read_u64(data, 0);
            format!("[PID {}] 🐛 DEBUG: Read Dentry Ptr: {:#x}", pid, ptr)
        }
        111 => {
            let ptr = read_u64(data, 0);
            format!("[PID {}] 🐛 DEBUG: Read Name Ptr: {:#x}", pid, ptr)
        }
        _ => return None,
    };
    Some(line)
}

#[cfg(target_os = "linux")]
pub(crate) fn log_monitor_event(event: &assay_common::MonitorEvent, args: &MonitorArgs) {
    if args.quiet {
        return;
    }
    if let Some(line) = format_monitor_event(event.event_type, event.pid, &event.data) {
        out(line);
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_blocked_net_payload, decode_file_blocked_payload, format_monitor_event};
    use assay_common::{EVENT_CONNECT, EVENT_FILE_BLOCKED, EVENT_OPENAT};

    #[test]
    fn decode_file_blocked_payload_reads_binary_layout() {
        let mut data = [0u8; 32];
        data[0..8].copy_from_slice(&42u64.to_ne_bytes());
        data[8..16].copy_from_slice(&7u64.to_ne_bytes());
        data[16..24].copy_from_slice(&99u64.to_ne_bytes());
        data[24..28].copy_from_slice(&1234u32.to_ne_bytes());

        let decoded = decode_file_blocked_payload(&data).expect("payload should decode");
        assert_eq!(decoded, (42, 7, 99, 1234));
    }

    #[test]
    fn decode_blocked_net_payload_reads_binary_layout() {
        let mut data = [0u8; 40];
        data[0..8].copy_from_slice(&42u64.to_ne_bytes());
        data[8..10].copy_from_slice(&2u16.to_ne_bytes());
        data[10..12].copy_from_slice(&443u16.to_ne_bytes());
        data[12..16].copy_from_slice(&[203, 0, 113, 7]);
        data[32..36].copy_from_slice(&9u32.to_ne_bytes());

        let decoded = decode_blocked_net_payload(&data).expect("payload should decode");

        assert_eq!(decoded, (42, "203.0.113.7".to_string(), 443, 9));
    }

    // The line shapes below are the producer contract that Plimsoll's capture scraper parses (see
    // plimsoll src/plimsoll/capture.py parse_monitor_lines). openat and connect are the two the
    // scraper extracts, so they are pinned in full; the rest pin the structural payload layout.
    fn cstr(s: &str) -> Vec<u8> {
        let mut v = s.as_bytes().to_vec();
        v.push(0);
        v
    }

    #[test]
    fn openat_event_formats_exact_path_line() {
        let line = format_monitor_event(EVENT_OPENAT, 4242, &cstr("/etc/passwd")).unwrap();
        assert_eq!(line, "[PID 4242] openat: /etc/passwd");
    }

    #[test]
    fn connect_event_formats_exact_sockaddr_hex_line() {
        let line = format_monitor_event(EVENT_CONNECT, 4242, &[0x02, 0x00, 0x00, 0x50]).unwrap();
        assert_eq!(line, "[PID 4242] connect sockaddr[0..32]=0x02000050");
    }

    #[test]
    fn file_blocked_event_formats_decoded_payload() {
        let mut data = [0u8; 32];
        data[0..8].copy_from_slice(&1u64.to_ne_bytes());
        data[8..16].copy_from_slice(&2u64.to_ne_bytes());
        data[16..24].copy_from_slice(&3u64.to_ne_bytes());
        data[24..28].copy_from_slice(&4u32.to_ne_bytes());
        let line = format_monitor_event(EVENT_FILE_BLOCKED, 4242, &data).unwrap();
        assert!(line.starts_with("[PID 4242] "), "{line}");
        assert!(
            line.ends_with(" BLOCKED FILE: dev=1 ino=2 cgroup=3 rule_id=4"),
            "{line}"
        );
    }

    #[test]
    fn blocked_net_event_formats_decoded_payload() {
        let mut data = [0u8; 40];
        data[0..8].copy_from_slice(&42u64.to_ne_bytes());
        data[8..10].copy_from_slice(&2u16.to_ne_bytes());
        data[10..12].copy_from_slice(&443u16.to_ne_bytes());
        data[12..16].copy_from_slice(&u32::from_ne_bytes([203, 0, 113, 7]).to_ne_bytes());
        data[32..36].copy_from_slice(&9u32.to_ne_bytes());

        let line = format_monitor_event(20, 4242, &data).unwrap();

        assert!(line.starts_with("[PID 4242] "), "{line}");
        assert!(
            line.ends_with(" BLOCKED NET: dst=203.0.113.7 port=443 cgroup=42 rule_id=9"),
            "{line}"
        );
    }

    #[test]
    fn allowed_file_event_formats_path_suffix() {
        let line = format_monitor_event(11, 4242, &cstr("/allowed/path")).unwrap();
        assert!(line.starts_with("[PID 4242] "), "{line}");
        assert!(line.ends_with(" ALLOWED FILE: /allowed/path"), "{line}");
    }

    #[test]
    fn unknown_event_type_produces_no_line() {
        assert_eq!(format_monitor_event(999, 4242, &[]), None);
    }

    #[test]
    fn short_buffers_do_not_panic_in_indexed_arms() {
        // The slice contract must stay bounds-safe: event types that read fixed offsets fall back
        // to zero on a short buffer instead of panicking (the live path always passes a full one).
        for event_type in [108u32, 110, 111, 112] {
            let line = format_monitor_event(event_type, 7, &[]).unwrap();
            assert!(line.starts_with("[PID 7] "), "{line}");
        }
    }
}
