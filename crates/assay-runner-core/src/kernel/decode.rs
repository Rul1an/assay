use assay_common::{
    MonitorEvent, EVENT_CONNECT, EVENT_CONNECT_BLOCKED, EVENT_EXEC, EVENT_FILE_BLOCKED,
    EVENT_OPENAT, EVENT_SENDMSG, EVENT_SENDTO,
};
use std::net::{Ipv4Addr, Ipv6Addr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DecodedKernelEvent {
    pub(super) kind: String,
    pub(super) value: Option<String>,
    pub(super) flags: Option<u64>,
    pub(super) mode: Option<u64>,
    pub(super) resolve: Option<u64>,
    pub(super) return_value: Option<i64>,
    pub(super) access_mode: Option<String>,
    pub(super) operation_flags: Vec<String>,
    pub(super) status: Option<String>,
}

pub(super) fn decode_monitor_event(event: &MonitorEvent) -> DecodedKernelEvent {
    match event.event_type {
        EVENT_OPENAT => decoded_open_event(event),
        EVENT_CONNECT => decoded_plain_event("connect", decode_sockaddr_endpoint(&event.data)),
        EVENT_SENDTO => decoded_plain_event("sendto", decode_sockaddr_endpoint(&event.data)),
        EVENT_SENDMSG => decoded_plain_event("sendmsg", decode_sockaddr_endpoint(&event.data)),
        EVENT_EXEC => decoded_plain_event("exec", decode_c_string(&event.data)),
        EVENT_FILE_BLOCKED => decoded_plain_event("file_blocked", decode_c_string(&event.data)),
        EVENT_CONNECT_BLOCKED => {
            decoded_plain_event("connect_blocked", decode_sockaddr_endpoint(&event.data))
        }
        other => decoded_plain_event(&format!("event_{other}"), None),
    }
}

fn decoded_plain_event(kind: &str, value: Option<String>) -> DecodedKernelEvent {
    DecodedKernelEvent {
        kind: kind.to_string(),
        value,
        flags: None,
        mode: None,
        resolve: None,
        return_value: None,
        access_mode: None,
        operation_flags: Vec::new(),
        status: None,
    }
}

fn decoded_open_event(event: &MonitorEvent) -> DecodedKernelEvent {
    let flags = event.flags;
    DecodedKernelEvent {
        kind: "openat".to_string(),
        value: decode_c_string(&event.data),
        flags: Some(flags),
        mode: Some(event.mode),
        resolve: (event.resolve != 0).then_some(event.resolve),
        return_value: Some(event.return_value),
        access_mode: Some(open_access_mode(flags).to_string()),
        operation_flags: open_operation_flags(flags),
        status: Some(
            if event.return_value < 0 {
                "error"
            } else {
                "success"
            }
            .to_string(),
        ),
    }
}

fn open_access_mode(flags: u64) -> &'static str {
    match flags & 0o3 {
        0 => "read",
        1 => "write",
        2 => "read_write",
        _ => "unknown",
    }
}

fn open_operation_flags(flags: u64) -> Vec<String> {
    let mut out = Vec::new();
    if flags & 0o100 != 0 {
        out.push("create".to_string());
    }
    if flags & 0o1000 != 0 {
        out.push("truncate".to_string());
    }
    if flags & 0o2000 != 0 {
        out.push("append".to_string());
    }
    if flags & 0o400 != 0 {
        out.push("exclusive".to_string());
    }
    out
}

fn decode_c_string(bytes: &[u8]) -> Option<String> {
    let end = bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len());
    if end == 0 {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes[..end]).to_string())
}

fn decode_sockaddr_endpoint(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 2 {
        return None;
    }
    let family = u16::from_ne_bytes(bytes[0..2].try_into().ok()?);
    match family {
        2 if bytes.len() >= 8 => {
            // AF_INET on Linux; monitor events are emitted by Linux eBPF code.
            let port = u16::from_be_bytes(bytes[2..4].try_into().ok()?);
            let addr = Ipv4Addr::new(bytes[4], bytes[5], bytes[6], bytes[7]);
            Some(format!("{addr}:{port}"))
        }
        10 if bytes.len() >= 28 => {
            // AF_INET6 on Linux; monitor events are emitted by Linux eBPF code.
            let port = u16::from_be_bytes(bytes[2..4].try_into().ok()?);
            let addr = Ipv6Addr::from(<[u8; 16]>::try_from(&bytes[8..24]).ok()?);
            Some(format!("[{addr}]:{port}"))
        }
        _ => None,
    }
}

/// Filter dynamic-loader and libc-internal telemetry from runner-spike evidence.
///
/// The monitor layer may observe these openat events, but they describe runtime
/// loader behavior rather than agent-attribution evidence. Keeping them in the
/// runner-spike bundle makes determinism depend on cargo RPATHs, libc locale
/// probing, Python interpreter bootstrap, and kernel introspection rather than
/// on the fixture's behavior.
pub(super) fn is_loader_telemetry_path(path: &str) -> bool {
    path == "/etc/ld.so.cache"
        || path == "/etc/localtime"
        || path == "/etc/ssl/openssl.cnf"
        || path == "/usr/pyvenv.cfg"
        || path == "/usr/bin/pyvenv.cfg"
        || path == "/usr/bin/python3._pth"
        || path == "/usr/bin/python3.12._pth"
        || path == "/usr/bin/pybuilddir.txt"
        || path.starts_with("/lib/")
        || path.starts_with("/lib32/")
        || path.starts_with("/lib64/")
        || path.starts_with("/usr/lib/")
        || path.starts_with("/usr/share/locale/")
        || path.contains("/node_modules/")
        || path.starts_with("/proc/")
        || path.starts_with("/sys/")
        || path.starts_with("/dev/")
        || (path.contains("/.rustup/toolchains/") && is_shared_object_path(path))
        || (path.contains("/target/")
            && (path.contains("/build/") || path.contains("/debug/") || path.contains("/release/"))
            && is_shared_object_path(path))
}

fn is_shared_object_path(path: &str) -> bool {
    path.ends_with(".so") || path.contains(".so.")
}
