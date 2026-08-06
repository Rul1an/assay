use assay_common::{
    CidrRuleValue, SocketEvent, RULE_ACTION_DENY, SOCKET_STATS_LEN, SOCKET_STAT_ALLOWED,
    SOCKET_STAT_BLOCKED_CIDR, SOCKET_STAT_BLOCKED_PORT, SOCKET_STAT_CHECKS,
    SOCKET_STAT_EVENTS_EMITTED, SOCKET_STAT_RINGBUF_DROPPED,
};
use aya_ebpf::{
    bindings::bpf_sock_addr,
    helpers::{bpf_get_current_cgroup_id, bpf_get_current_pid_tgid, bpf_ktime_get_ns},
    macros::{cgroup_sock_addr, map},
    maps::lpm_trie::Key,
    maps::{Array, HashMap, LpmTrie, RingBuf},
    programs::SockAddrContext,
    EbpfContext,
};

const MAX_CIDR_RULES: u32 = 1024;
const MAX_PORT_RULES: u32 = 256;

const EVENT_CONNECT_BLOCKED: u32 = 20;

/// CIDR rules carry the action to enforce and the id of the policy rule that
/// produced them; see [`CidrRuleValue`] for why the value needs both. Populated
/// from userspace by `assay_monitor::loader::set_tier1_rules`.
#[map]
static CIDR_RULES_V4: LpmTrie<[u8; 4], CidrRuleValue> =
    LpmTrie::with_max_entries(MAX_CIDR_RULES, 0);

#[map]
static CIDR_RULES_V6: LpmTrie<[u8; 16], CidrRuleValue> =
    LpmTrie::with_max_entries(MAX_CIDR_RULES, 0);

#[map]
static DENY_PORTS: HashMap<u16, u32> = HashMap::with_max_entries(MAX_PORT_RULES, 0);

#[map]
static ALLOW_PORTS: HashMap<u16, u8> = HashMap::with_max_entries(MAX_PORT_RULES, 0);

#[map]
static SOCKET_EVENTS: RingBuf = RingBuf::with_byte_size(128 * 1024, 0);

#[map]
static SOCKET_STATS: Array<u64> = Array::with_max_entries(SOCKET_STATS_LEN, 0);

#[cgroup_sock_addr(connect4)]
pub fn connect4_hook(ctx: SockAddrContext) -> i32 {
    match try_connect4(&ctx) {
        Ok(allow) => {
            if allow {
                1
            } else {
                0
            }
        }
        Err(_) => 1,
    }
}

#[inline(always)]
fn try_connect4(ctx: &SockAddrContext) -> Result<bool, i64> {
    inc_stat(SOCKET_STAT_CHECKS);

    // SAFETY: `bpf_get_current_cgroup_id` returns a scalar cgroup id from the
    // verifier-provided helper; the result is not dereferenced.
    let cgroup_id = unsafe { bpf_get_current_cgroup_id() };
    // SAFETY: `ctx.as_ptr()` is provided by the cgroup sockaddr hook and points
    // to a `bpf_sock_addr` for the duration of this hook invocation.
    let sock_addr = unsafe { &*(ctx.as_ptr() as *const bpf_sock_addr) };
    let dst_port = u16::from_be(sock_addr.user_port as u16);

    // SAFETY: DENY_PORTS is an eBPF map owned by this program. The destination
    // port key is a scalar copied from the hook context.
    if let Some(&rule_id) = unsafe { DENY_PORTS.get(&dst_port) } {
        emit_socket_event(
            EVENT_CONNECT_BLOCKED,
            cgroup_id,
            2, // IPv4
            dst_port,
            sock_addr.user_ip4,
            &[0u8; 16],
            rule_id,
            // Membership in DENY_PORTS is itself the deny decision; the map stores
            // only the rule id, so the action is implied rather than looked up.
            RULE_ACTION_DENY,
        );
        inc_stat(SOCKET_STAT_BLOCKED_PORT);
        return Ok(false);
    }

    // SAFETY: ALLOW_PORTS is an eBPF map owned by this program. A missing key
    // means no explicit allow rule matched this destination port.
    if unsafe { ALLOW_PORTS.get(&dst_port).is_some() } {
        inc_stat(SOCKET_STAT_ALLOWED);
        return Ok(true);
    }

    // Longest-prefix match against the compiled CIDR rules. A match carries the
    // action to enforce and the id of the rule that produced it, so the emitted
    // event attributes the block to that specific policy rule rather than to a
    // constant. An allow match falls through to the same allow path as no match.
    let key = Key::new(32, sock_addr.user_ip4.to_ne_bytes());
    if let Some(&rule) = CIDR_RULES_V4.get(&key) {
        if rule.action == RULE_ACTION_DENY {
            emit_socket_event(
                EVENT_CONNECT_BLOCKED,
                cgroup_id,
                2, // IPv4
                dst_port,
                sock_addr.user_ip4,
                &[0u8; 16],
                rule.rule_id,
                rule.action,
            );
            inc_stat(SOCKET_STAT_BLOCKED_CIDR);
            return Ok(false);
        }
    }

    inc_stat(SOCKET_STAT_ALLOWED);
    Ok(true)
}

#[cgroup_sock_addr(connect6)]
pub fn connect6_hook(ctx: SockAddrContext) -> i32 {
    match try_connect6(&ctx) {
        Ok(allow) => {
            if allow {
                1
            } else {
                0
            }
        }
        Err(_) => 1,
    }
}

#[inline(always)]
fn try_connect6(ctx: &SockAddrContext) -> Result<bool, i64> {
    inc_stat(SOCKET_STAT_CHECKS);

    // SAFETY: `bpf_get_current_cgroup_id` returns a scalar cgroup id from the
    // verifier-provided helper; the result is not dereferenced.
    let cgroup_id = unsafe { bpf_get_current_cgroup_id() };
    // SAFETY: `ctx.as_ptr()` is provided by the cgroup sockaddr hook and points
    // to a `bpf_sock_addr` for the duration of this hook invocation.
    let sock_addr = unsafe { &*(ctx.as_ptr() as *const bpf_sock_addr) };
    let dst_port = u16::from_be(sock_addr.user_port as u16);
    let dst_addr = sock_addr.user_ip6;

    // SAFETY: DENY_PORTS is an eBPF map owned by this program. The destination
    // port key is a scalar copied from the hook context.
    if let Some(&rule_id) = unsafe { DENY_PORTS.get(&dst_port) } {
        // SAFETY: `[u32; 4]` and `[u8; 16]` have the same size, and every byte
        // pattern is valid for `u8`; the bytes are copied into the event payload.
        let dst_addr_bytes = unsafe { core::mem::transmute::<[u32; 4], [u8; 16]>(dst_addr) };
        emit_socket_event(
            EVENT_CONNECT_BLOCKED,
            cgroup_id,
            10, // IPv6
            dst_port,
            0,
            &dst_addr_bytes,
            rule_id,
            // See the connect4 port path: DENY_PORTS membership implies the action.
            RULE_ACTION_DENY,
        );
        inc_stat(SOCKET_STAT_BLOCKED_PORT);
        return Ok(false);
    }

    // SAFETY: ALLOW_PORTS is an eBPF map owned by this program. A missing key
    // means no explicit allow rule matched this destination port.
    if unsafe { ALLOW_PORTS.get(&dst_port).is_some() } {
        inc_stat(SOCKET_STAT_ALLOWED);
        return Ok(true);
    }

    // SAFETY: `[u32; 4]` and `[u8; 16]` have the same size, and every byte
    // pattern is valid for `u8`; the bytes are used immediately as an LPM key.
    let ip6_bytes = unsafe { core::mem::transmute::<[u32; 4], [u8; 16]>(dst_addr) };
    let key = Key::new(128, ip6_bytes);
    if let Some(&rule) = CIDR_RULES_V6.get(&key) {
        if rule.action == RULE_ACTION_DENY {
            emit_socket_event(
                EVENT_CONNECT_BLOCKED,
                cgroup_id,
                10, // IPv6
                dst_port,
                0,
                &ip6_bytes,
                rule.rule_id,
                rule.action,
            );
            inc_stat(SOCKET_STAT_BLOCKED_CIDR);
            return Ok(false);
        }
    }

    inc_stat(SOCKET_STAT_ALLOWED);
    Ok(true)
}

#[inline(always)]
fn inc_stat(index: u32) {
    if let Some(val) = SOCKET_STATS.get_ptr_mut(index) {
        // SAFETY: `val` points to a mutable counter returned by the eBPF stats
        // array; the verifier checks map bounds for the supplied index.
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
        // SAFETY: `event.as_mut_ptr()` points to a reserved `SocketEvent`
        // ring-buffer entry initialized below before submit.
        let ev = unsafe { &mut *event.as_mut_ptr() };
        ev.event_type = event_type;
        ev.pid = (bpf_get_current_pid_tgid() >> 32) as u32;
        // SAFETY: `bpf_ktime_get_ns` returns a scalar timestamp from the
        // verifier-provided helper; the result is not dereferenced.
        ev.timestamp_ns = unsafe { bpf_ktime_get_ns() };
        ev.cgroup_id = cgroup_id;
        ev.family = family;
        ev.port = port;
        ev.addr_v4 = addr_v4;
        ev.rule_id = rule_id;
        ev.action = action;

        ev.addr_v6.copy_from_slice(addr_v6);
        event.submit(0);
        inc_stat(SOCKET_STAT_EVENTS_EMITTED);
    } else {
        inc_stat(SOCKET_STAT_RINGBUF_DROPPED);
    }
}
