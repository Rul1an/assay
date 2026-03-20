#[cfg(target_os = "linux")]
use crate::cli::commands::monitor::MonitorArgs;

#[cfg(target_os = "linux")]
pub(crate) fn out(message: impl AsRef<str>) {
    println!("{}", message.as_ref());
}

pub(crate) fn err(message: impl AsRef<str>) {
    eprintln!("{}", message.as_ref());
}

#[cfg(target_os = "linux")]
pub(crate) fn decode_utf8_cstr(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    String::from_utf8_lossy(&data[..end]).to_string()
}

#[cfg(target_os = "linux")]
pub(crate) fn dump_prefix_hex(data: &[u8], n: usize) -> String {
    data.iter()
        .take(n)
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join("")
}

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
pub(crate) fn log_monitor_event(event: &assay_common::MonitorEvent, args: &MonitorArgs) {
    use assay_common::{EVENT_CONNECT, EVENT_FILE_BLOCKED, EVENT_OPENAT};

    if args.quiet {
        return;
    }

    match event.event_type {
        EVENT_OPENAT => println!(
            "[PID {}] openat: {}",
            event.pid,
            decode_utf8_cstr(&event.data)
        ),
        EVENT_CONNECT => println!(
            "[PID {}] connect sockaddr[0..32]=0x{}",
            event.pid,
            dump_prefix_hex(&event.data, 32)
        ),
        EVENT_FILE_BLOCKED => match decode_file_blocked_payload(&event.data) {
            Some((dev, ino, cgroup_id, rule_id)) => println!(
                "[PID {}] 🛡️ BLOCKED FILE: dev={} ino={} cgroup={} rule_id={}",
                event.pid, dev, ino, cgroup_id, rule_id
            ),
            None => println!(
                "[PID {}] 🛡️ BLOCKED FILE: 0x{}",
                event.pid,
                dump_prefix_hex(&event.data, 32)
            ),
        },
        11 => println!(
            "[PID {}] 🟢 ALLOWED FILE: {}",
            event.pid,
            decode_utf8_cstr(&event.data)
        ),
        20 => println!(
            "[PID {}] 🛡️ BLOCKED NET : {}",
            event.pid,
            dump_prefix_hex(&event.data, 20)
        ),
        112 => {
            let dev_bytes: [u8; 8] = event.data[0..8].try_into().unwrap_or([0; 8]);
            let ino_bytes: [u8; 8] = event.data[8..16].try_into().unwrap_or([0; 8]);
            let gen_bytes: [u8; 4] = event.data[16..20].try_into().unwrap_or([0; 4]);

            let dev = u64::from_ne_bytes(dev_bytes);
            let ino = u64::from_ne_bytes(ino_bytes);
            let gen = u32::from_ne_bytes(gen_bytes);

            println!(
                "[PID {}] 🔒 INODE RESOLVED: dev={} (0x{:x}) ino={} gen={}",
                event.pid, dev, dev, ino, gen
            );
        }
        101..=104 => {
            let chunk_idx = event.event_type - 101;
            let start_offset = chunk_idx * 64;
            let dump = dump_prefix_hex(&event.data, 64);
            println!(
                "[PID {}] 🔍 STRUCT DUMP Part {} (Offset {}-{}): {}",
                event.pid,
                chunk_idx + 1,
                start_offset,
                start_offset + 64,
                dump
            );
        }
        105 => {
            let path = decode_utf8_cstr(&event.data);
            println!(
                "[PID {}] 📂 FILE OPEN (Manual Resolution): {}",
                event.pid, path
            );
        }
        106 => {
            println!("[PID {}] 🐛 DEBUG: Dentry Pointer NULL", event.pid);
        }
        107 => {
            println!("[PID {}] 🐛 DEBUG: Name Pointer NULL", event.pid);
        }
        108 => {
            println!(
                "[PID {}] 🐛 DEBUG: LSM Hook Entry (MonitorAll={})",
                event.pid, event.data[0]
            );
        }
        109 => {
            println!("[PID {}] 🐛 DEBUG: Passed Monitor Check", event.pid);
        }
        110 => {
            let ptr = u64::from_ne_bytes(event.data[0..8].try_into().unwrap());
            println!("[PID {}] 🐛 DEBUG: Read Dentry Ptr: {:#x}", event.pid, ptr);
        }
        111 => {
            let ptr = u64::from_ne_bytes(event.data[0..8].try_into().unwrap());
            println!("[PID {}] 🐛 DEBUG: Read Name Ptr: {:#x}", event.pid, ptr);
        }
        _ => if args.monitor_all || !args.quiet {},
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::decode_file_blocked_payload;

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
}
