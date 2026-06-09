//! Bridge from assay-cli's string network policy (`crate::policy::NetPolicy`) to the assay-policy
//! Landlock TCP-connect compile target.
//!
//! The sandbox's Landlock-net enforcement is allowlist-only over TCP-connect ports. This module
//! parses the free-form `net.allow` / `net.deny` strings into a structured
//! `assay_policy::tiers::NetworkPolicy` and runs `compile_landlock_net`, so the enforcement path
//! only ever applies an explicit, Landlock-expressible port allowlist and fails closed on everything
//! else. Forms the structured policy cannot even represent as a TCP-connect ALLOW (a non-TCP
//! protocol, a port range, or a host/wildcard on the allow side) are rejected here at parse time;
//! IP/CIDR, any deny rule, and port 0 are rejected by the compiler.

use assay_policy::tiers::{compile_landlock_net, LandlockRejectReason, NetworkPolicy};

/// Why a network policy is not enforceable as a Landlock TCP-connect allowlist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetReject {
    pub reason: NetRejectReason,
    pub entry: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetRejectReason {
    /// A non-TCP protocol on an allow entry (e.g. `udp/53`, `quic/443`). Landlock-net is TCP-only.
    Protocol,
    /// A port range (e.g. `443-445`). Only an explicit port list is accepted, never a range.
    Range,
    /// An IP literal or CIDR. Landlock network rules are ports, not addresses.
    Cidr,
    /// Any deny rule. Landlock is allowlist-only.
    NegativeDeny,
    /// A host:port / hostname / wildcard. Landlock has no endpoint identity.
    Destination,
    /// Port 0: maps to the kernel-assigned ephemeral range and would widen the allowlist.
    PortZero,
    /// An allow entry that is none of the understood forms.
    Malformed,
}

impl NetRejectReason {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            NetRejectReason::Protocol => "protocol_not_tcp",
            NetRejectReason::Range => "port_range_not_allowed",
            NetRejectReason::Cidr => "cidr_not_expressible",
            NetRejectReason::NegativeDeny => "negative_deny_not_expressible",
            NetRejectReason::Destination => "destination_not_expressible",
            NetRejectReason::PortZero => "port_zero_not_allowed",
            NetRejectReason::Malformed => "malformed_net_entry",
        }
    }
}

fn map_compile_reason(reason: LandlockRejectReason) -> NetRejectReason {
    match reason {
        LandlockRejectReason::Cidr => NetRejectReason::Cidr,
        LandlockRejectReason::NegativeDeny => NetRejectReason::NegativeDeny,
        LandlockRejectReason::Destination => NetRejectReason::Destination,
        LandlockRejectReason::PortZero => NetRejectReason::PortZero,
    }
}

fn looks_like_cidr_or_ip(s: &str) -> bool {
    // An address-shaped token: contains a CIDR slash, or parses as a bare IP.
    if s.contains('/') {
        return true;
    }
    s.parse::<std::net::IpAddr>().is_ok()
}

/// Parse a single allow entry into either a TCP-connect port (pushed to `allow_ports`, including 0
/// so the compiler can reject it) or a structured rejection. CIDR-shaped tokens go to `allow_cidrs`
/// so the compiler reports them uniformly.
fn classify_allow(entry: &str, network: &mut NetworkPolicy) -> Option<NetReject> {
    let token = entry.trim();

    // Bare port.
    if let Ok(port) = token.parse::<u16>() {
        network.allow_ports.push(port);
        return None;
    }

    // `tcp/<port>` or `tcp:<port>`.
    if let Some(rest) = token
        .strip_prefix("tcp/")
        .or_else(|| token.strip_prefix("tcp:"))
    {
        return match rest.parse::<u16>() {
            Ok(port) => {
                network.allow_ports.push(port);
                None
            }
            Err(_) => Some(NetReject {
                reason: NetRejectReason::Malformed,
                entry: entry.to_string(),
            }),
        };
    }

    // An alphabetic scheme before `/` or `:` (e.g. `udp/53`, `quic/443`, `http:80`) is a non-TCP
    // protocol. `tcp/` is already consumed above, and address forms like `10.0.0.0/8` have a numeric
    // (non-alphabetic) scheme, so they fall through to the CIDR check below.
    for sep in ['/', ':'] {
        if let Some((proto, _)) = token.split_once(sep) {
            if !proto.is_empty() && proto.chars().all(|c| c.is_ascii_alphabetic()) {
                return Some(NetReject {
                    reason: NetRejectReason::Protocol,
                    entry: entry.to_string(),
                });
            }
        }
    }

    // Port range like `443-445`.
    if token
        .split_once('-')
        .is_some_and(|(a, b)| a.parse::<u16>().is_ok() && b.parse::<u16>().is_ok())
    {
        return Some(NetReject {
            reason: NetRejectReason::Range,
            entry: entry.to_string(),
        });
    }

    // IP literal or CIDR.
    if looks_like_cidr_or_ip(token) {
        network.allow_cidrs.push(token.to_string());
        return None;
    }

    // host:port or hostname/wildcard on the allow side has no representable home.
    if token.contains(':') || token.chars().any(|c| c.is_ascii_alphabetic()) {
        return Some(NetReject {
            reason: NetRejectReason::Destination,
            entry: entry.to_string(),
        });
    }

    Some(NetReject {
        reason: NetRejectReason::Malformed,
        entry: entry.to_string(),
    })
}

/// Plan the Landlock TCP-connect allowlist for a network policy, or fail closed with every reason it
/// is not Landlock-enforceable. An empty allowlist is valid (deny-all TCP connect).
///
/// # Errors
/// Returns every [`NetReject`] that applies; a non-empty list means the sandbox must not enforce
/// Landlock-net for this policy.
pub fn plan_landlock_net_ports(net: &crate::policy::NetPolicy) -> Result<Vec<u16>, Vec<NetReject>> {
    let mut network = NetworkPolicy::default();
    let mut rejects: Vec<NetReject> = Vec::new();

    for entry in &net.allow {
        if let Some(reject) = classify_allow(entry, &mut network) {
            rejects.push(reject);
        }
    }

    // Any deny entry is a negative rule; route it into the structured policy so the compiler reports
    // it (and add the literal entry for diagnostics).
    for entry in &net.deny {
        let token = entry.trim();
        if token.parse::<u16>().is_ok() {
            network.deny_ports.push(token.parse().unwrap());
        } else if looks_like_cidr_or_ip(token) {
            network.deny_cidrs.push(token.to_string());
        } else {
            network.deny_destinations.push(token.to_string());
        }
    }

    match compile_landlock_net(&network) {
        Ok(target) => {
            if rejects.is_empty() {
                Ok(target.allowed_connect_tcp_ports)
            } else {
                Err(rejects)
            }
        }
        Err(compile_rejects) => {
            for r in compile_rejects {
                rejects.push(NetReject {
                    reason: map_compile_reason(r.reason),
                    entry: r.detail,
                });
            }
            Err(rejects)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::NetPolicy;

    fn reasons(err: &[NetReject]) -> Vec<NetRejectReason> {
        let mut r: Vec<NetRejectReason> = err.iter().map(|e| e.reason).collect();
        r.sort_by_key(|x| x.as_str());
        r.dedup();
        r
    }

    fn allow(entries: &[&str]) -> NetPolicy {
        NetPolicy {
            allow: entries.iter().map(|s| s.to_string()).collect(),
            deny: vec![],
        }
    }

    #[test]
    fn accepts_bare_and_tcp_prefixed_ports_sorted_deduped() {
        let ports = plan_landlock_net_ports(&allow(&["443", "tcp/80", "443"])).unwrap();
        assert_eq!(ports, vec![80, 443]);
    }

    #[test]
    fn accepts_empty_as_deny_all() {
        assert_eq!(
            plan_landlock_net_ports(&allow(&[])).unwrap(),
            Vec::<u16>::new()
        );
    }

    #[test]
    fn rejects_udp_and_quic_as_protocol() {
        for e in ["udp/53", "quic/443"] {
            let err = plan_landlock_net_ports(&allow(&[e])).unwrap_err();
            assert_eq!(reasons(&err), vec![NetRejectReason::Protocol], "{e}");
        }
    }

    #[test]
    fn rejects_port_range() {
        let err = plan_landlock_net_ports(&allow(&["443-445"])).unwrap_err();
        assert_eq!(reasons(&err), vec![NetRejectReason::Range]);
    }

    #[test]
    fn rejects_ip_and_cidr() {
        for e in ["10.0.0.0/8", "203.0.113.10", "::1"] {
            let err = plan_landlock_net_ports(&allow(&[e])).unwrap_err();
            assert_eq!(reasons(&err), vec![NetRejectReason::Cidr], "{e}");
        }
    }

    #[test]
    fn rejects_host_and_wildcard_destinations() {
        for e in ["example.com:443", "*.internal"] {
            let err = plan_landlock_net_ports(&allow(&[e])).unwrap_err();
            assert_eq!(reasons(&err), vec![NetRejectReason::Destination], "{e}");
        }
    }

    #[test]
    fn rejects_port_zero() {
        let err = plan_landlock_net_ports(&allow(&["0"])).unwrap_err();
        assert_eq!(reasons(&err), vec![NetRejectReason::PortZero]);
    }

    #[test]
    fn rejects_any_deny_as_negative() {
        let net = NetPolicy {
            allow: vec!["443".to_string()],
            deny: vec!["4444".to_string()],
        };
        let err = plan_landlock_net_ports(&net).unwrap_err();
        assert_eq!(reasons(&err), vec![NetRejectReason::NegativeDeny]);
    }

    #[test]
    fn reason_ids_are_stable() {
        assert_eq!(NetRejectReason::Protocol.as_str(), "protocol_not_tcp");
        assert_eq!(NetRejectReason::Range.as_str(), "port_range_not_allowed");
        assert_eq!(NetRejectReason::PortZero.as_str(), "port_zero_not_allowed");
    }
}
