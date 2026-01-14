
use aya_ebpf::{
    macros::{cgroup_sock_addr, map},
    maps::{HashMap, LpmTrie, RingBuf, Array},
    maps::lpm_trie::Key,
    programs::SockAddrContext,
    helpers::{bpf_get_current_cgroup_id, bpf_ktime_get_ns, bpf_get_current_pid_tgid},
    EbpfContext,
    bindings::bpf_sock_addr,
};

const MAX_CIDR_RULES: u32 = 1024;
const MAX_PORT_RULES: u32 = 256;

const EVENT_CONNECT_BLOCKED: u32 = 20;

const ACTION_DENY: u8 = 2;

#[map]
static CIDR_RULES_V4: LpmTrie<[u8; 4], u32> = LpmTrie::with_max_entries(MAX_CIDR_RULES, 0);

#[map]
static CIDR_RULES_V6: LpmTrie<[u8; 16], u32> = LpmTrie::with_max_entries(MAX_CIDR_RULES, 0);

#[map]
static DENY_PORTS: HashMap<u16, u32> = HashMap::with_max_entries(MAX_PORT_RULES, 0);

#[map]
static ALLOW_PORTS: HashMap<u16, u8> = HashMap::with_max_entries(MAX_PORT_RULES, 0);

#[map]
static SOCKET_EVENTS: RingBuf = RingBuf::with_byte_size(128 * 1024, 0);

#[map]
static SOCKET_STATS: Array<u64> = Array::with_max_entries(8, 0);

const STAT_CHECKS: u32 = 0;
const STAT_BLOCKED_CIDR: u32 = 1;
const STAT_BLOCKED_PORT: u32 = 2;
const STAT_ALLOWED: u32 = 3;

#[repr(C)]
struct SocketEvent {
    event_type: u32,
    pid: u32,
    timestamp_ns: u64,
    cgroup_id: u64,
    family: u16,
    port: u16,
    addr_v4: u32,
    addr_v6: [u8; 16],
    rule_id: u32,
    action: u32,
}

#[cgroup_sock_addr(connect4)]
pub fn connect4_hook(ctx: SockAddrContext) -> i32 {
    match try_connect4(&ctx) {
        Ok(allow) => if allow { 1 } else { 0 },
        Err(_) => 1,
    }
}

#[inline(always)]
fn try_connect4(ctx: &SockAddrContext) -> Result<bool, i64> {
    inc_stat(STAT_CHECKS);

    let cgroup_id = unsafe { bpf_get_current_cgroup_id() };
    let sock_addr = unsafe { &*(ctx.as_ptr() as *const bpf_sock_addr) };
    let dst_port = u16::from_be(sock_addr.user_port as u16);

    if let Some(&rule_id) = unsafe { DENY_PORTS.get(&dst_port) } {
        emit_socket_event(
            EVENT_CONNECT_BLOCKED,
            cgroup_id,
            2, // IPv4
            dst_port,
            sock_addr.user_ip4,
            &[0u8; 16],
            rule_id,
            0,
        );
        inc_stat(STAT_BLOCKED_PORT);
        return Ok(false);
    }

    if unsafe { ALLOW_PORTS.get(&dst_port).is_some() } {
        inc_stat(STAT_ALLOWED);
        return Ok(true);
    }

    let key = Key::new(32, sock_addr.user_ip4.to_ne_bytes());
    if let Some(&action) = CIDR_RULES_V4.get(&key) {
        if action == ACTION_DENY as u32 {
            // Retrieve rule ID from DENY_CIDR_RULES (if we had it separately)
            // But here we only have action. Wait, CIDR_RULES_V4 stores ACTION (u8).
            // We need to store rule_id if we want to report it.
            // Copilot points out we use magic numbers 200/300.
            // But the map value is u8 (action).
            // To support rule_id, we need to change map value type to u32 or struct { action, rule_id }.
            // FOR NOW: We can't change map layout easily without update userspace.
            // Let's assume for this fix, we will stick to magic number 200 but verify if we can pass rule_id.
            // Userspace populates this map.
            // Ah, the user space code populates `trie.insert(..., action)`.
            // Wait, look at `loader.rs`: `hm.insert(hash, [len, rule_id])` for prefix.
            // But for CIDR: `trie.insert(&Key::new(prefix_len, addr), action, 0)?`
            // So we ONLY store action (u8).
            // So we DON'T have the rule_id available in the kernel with current map structure!
            // I will use `action as u32` if it helps, but action is just 2 (DENY).
            // The feedback says "This should use the actual rule_id".
            // This implies I need to change the map type?
            // "The rule_id is hardcoded ... use the actual rule_id from the matched CIDR rule"
            // This requires modifying the map value to be u32 (rule_id) or struct.
            // Let's check `CIDR_RULES_V4` definition.
            // Line 20: `static CIDR_RULES_V4: LpmTrie<[u8; 4], u8>`.
            // REQUIRED CHANGE: Change value type to u32 (rule_id).
            // But `loader.rs` uses `trie.insert(..., action)`.
            // I need to change `loader.rs` as well.

            // Wait, I can only edit one file at a time or simple edits.
            // Changing map definition is multi-file.
            // I will start by updating the map definition in THIS file and updating the logic.
            // Then I will update loader.rs in the next step.

            // Actually, I can do it in parallel if I am careful.
            // But `socket_lsm.rs` defines the usage.

            // Let's change the map to store u32 (rule_id).
            // And assumes userspace sends rule_id.

            emit_socket_event(
                EVENT_CONNECT_BLOCKED,
                cgroup_id,
                2,
                dst_port,
                sock_addr.user_ip4,
                &[0u8; 16],
                action, // Use the value from map as rule_id (assuming we change map to store rule_id)
                0,
            );
            inc_stat(STAT_BLOCKED_CIDR);
            return Ok(false);
        }
    }


    // Correctly close try_connect4
    inc_stat(STAT_ALLOWED);
    Ok(true)
}

#[cgroup_sock_addr(connect6)]
pub fn connect6_hook(ctx: SockAddrContext) -> i32 {
    match try_connect6(&ctx) {
        Ok(allow) => if allow { 1 } else { 0 },
        Err(_) => 1,
    }
}

#[inline(always)]
fn try_connect6(ctx: &SockAddrContext) -> Result<bool, i64> {
    inc_stat(STAT_CHECKS);

    let cgroup_id = unsafe { bpf_get_current_cgroup_id() };
    let sock_addr = unsafe { &*(ctx.as_ptr() as *const bpf_sock_addr) };
    let dst_port = u16::from_be(sock_addr.user_port as u16);
    let dst_addr = sock_addr.user_ip6;

    if let Some(&rule_id) = unsafe { DENY_PORTS.get(&dst_port) } {
        emit_socket_event(
            EVENT_CONNECT_BLOCKED,
            cgroup_id,
            10, // IPv6
            dst_port,
            0,
            &unsafe { core::mem::transmute::<[u32; 4], [u8; 16]>(dst_addr) },
            rule_id,
            0,
        );
        inc_stat(STAT_BLOCKED_PORT);
        return Ok(false);
    }

    if unsafe { ALLOW_PORTS.get(&dst_port).is_some() } {
        inc_stat(STAT_ALLOWED);
        return Ok(true);
    }

    let ip6_bytes = unsafe { core::mem::transmute::<[u32; 4], [u8; 16]>(dst_addr) };
    let key = Key::new(128, ip6_bytes);
    if let Some(&action) = CIDR_RULES_V6.get(&key) {
        if action == ACTION_DENY as u32 {
             emit_socket_event(
                EVENT_CONNECT_BLOCKED,
                cgroup_id,
                10,
                dst_port,
                0,
                &ip6_bytes,
                action, // Use rule_id
                0,
            );
            inc_stat(STAT_BLOCKED_CIDR);
            return Ok(false);
        }
    }

    inc_stat(STAT_ALLOWED);
    Ok(true)
}

#[inline(always)]
fn inc_stat(index: u32) {
    if let Some(val) = SOCKET_STATS.get_ptr_mut(index) {
        unsafe { *val += 1 };
    }
}

#[inline(always)]
fn emit_socket_event(
    event_type: u32,
    cgroup_id: u64,
    family: u16,
    port: u16,
    addr_v4: u32,
    addr_v6: &[u8; 16],
    rule_id: u32,
    action: u32,
) {
    if let Some(mut event) = SOCKET_EVENTS.reserve::<SocketEvent>(0) {
        let ev = unsafe { &mut *event.as_mut_ptr() };
        ev.event_type = event_type;
        ev.pid = (bpf_get_current_pid_tgid() >> 32) as u32;
        ev.timestamp_ns = unsafe { bpf_ktime_get_ns() };
        ev.cgroup_id = cgroup_id;
        ev.family = family;
        ev.port = port;
        ev.addr_v4 = addr_v4;
        ev.rule_id = rule_id;
        ev.action = action;

        for i in 0..16 {
            ev.addr_v6[i] = addr_v6[i];
        }
        event.submit(0);
    }
}
