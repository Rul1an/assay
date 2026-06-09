//! Landlock TCP-connect port-allowlist compile target.
//!
//! This is the second, independent enforcement route beside the connect4/eBPF egress path. It is
//! deliberately narrow: Landlock network rules are an ALLOWLIST over TCP ports. A handled access
//! right is denied by default once enforced, and rules grant exceptions for specific TCP ports.
//! Landlock has no notion of IP/CIDR, hostname, wildcard, endpoint identity, negative/deny rules,
//! UDP, or QUIC, so this compiler refuses to produce a target for anything it cannot faithfully
//! enforce, rather than silently dropping the unenforceable part.
//!
//! Scope: this is the compile target and its tests only. No sandbox applies it yet (that is the
//! later, gated enforcement step). The eBPF tier compiler (`compile`) is unchanged: network deny
//! rules still go to Tier 1 there. This target is a separate, additive path.
//!
//! Representability boundary: the policy model (`NetworkPolicy`) has no protocol selector and no
//! port-range syntax. `allow_ports` is `Vec<u16>`, so a port range cannot be expressed (rejected by
//! construction), and there is no way to request UDP/QUIC/non-TCP (TCP-connect by construction).
//! The runtime rejections below cover every Landlock-inexpressible shape the model CAN represent:
//! IP/CIDR rules, negative/deny rules, and host/wildcard destinations, plus port 0. If the policy
//! model later gains a protocol or range field, this compiler must add explicit rejections for them
//! at that point (an explicit extension, never a silent acceptance).

use super::types::NetworkPolicy;

/// Why a policy cannot compile to a Landlock TCP-connect allowlist. Machine-readable; each reason is
/// a shape Landlock cannot enforce, so producing a target would overclaim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LandlockRejectReason {
    /// IP/CIDR allow or deny rules: Landlock network rules are ports, not addresses.
    Cidr,
    /// A negative/deny rule (port or destination deny): Landlock is allowlist-only.
    NegativeDeny,
    /// A host:port / hostname / wildcard destination: Landlock has no endpoint identity.
    Destination,
    /// Port 0 in the allowlist: `bind(0)`/connect semantics map to the kernel-assigned ephemeral
    /// range, which would silently widen the allowlist.
    PortZero,
}

impl LandlockRejectReason {
    /// Stable snake_case id for the reason (machine-readable, never prose only).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            LandlockRejectReason::Cidr => "cidr_not_expressible",
            LandlockRejectReason::NegativeDeny => "negative_deny_not_expressible",
            LandlockRejectReason::Destination => "destination_not_expressible",
            LandlockRejectReason::PortZero => "port_zero_not_allowed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LandlockRejection {
    pub reason: LandlockRejectReason,
    pub detail: String,
}

/// A compiled Landlock TCP-connect target: the explicit set of allowed connect ports. An empty set
/// is valid and means "deny all TCP connects" (handle the right, add no port exceptions).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LandlockNetTarget {
    /// Allowed TCP-connect ports, sorted and de-duplicated, guaranteed non-zero.
    pub allowed_connect_tcp_ports: Vec<u16>,
}

/// Compile the network policy into a Landlock TCP-connect allowlist target, or fail closed with the
/// full set of reasons it cannot be expressed. All violations are collected, not just the first, so
/// the caller sees every reason a policy is not Landlock-enforceable.
///
/// # Errors
/// Returns every [`LandlockRejection`] that applies; non-empty means the policy is not expressible
/// as a Landlock TCP-connect allowlist and no target is produced.
pub fn compile_landlock_net(
    network: &NetworkPolicy,
) -> Result<LandlockNetTarget, Vec<LandlockRejection>> {
    let mut rejections = Vec::new();

    if !network.allow_cidrs.is_empty() || !network.deny_cidrs.is_empty() {
        rejections.push(LandlockRejection {
            reason: LandlockRejectReason::Cidr,
            detail: "Landlock network rules are TCP ports, not IP/CIDR addresses".to_string(),
        });
    }
    if !network.deny_ports.is_empty() {
        rejections.push(LandlockRejection {
            reason: LandlockRejectReason::NegativeDeny,
            detail: "Landlock is allowlist-only; port-deny rules cannot be expressed".to_string(),
        });
    }
    if !network.deny_destinations.is_empty() {
        rejections.push(LandlockRejection {
            reason: LandlockRejectReason::Destination,
            detail: "Landlock has no endpoint identity; host/wildcard destinations cannot be \
                     expressed"
                .to_string(),
        });
    }
    if network.allow_ports.contains(&0) {
        rejections.push(LandlockRejection {
            reason: LandlockRejectReason::PortZero,
            detail: "port 0 maps to the kernel-assigned ephemeral range and would widen the \
                     allowlist"
                .to_string(),
        });
    }

    if !rejections.is_empty() {
        return Err(rejections);
    }

    let mut ports: Vec<u16> = network.allow_ports.clone();
    ports.sort_unstable();
    ports.dedup();
    Ok(LandlockNetTarget {
        allowed_connect_tcp_ports: ports,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn net() -> NetworkPolicy {
        NetworkPolicy::default()
    }

    fn reasons(err: &[LandlockRejection]) -> Vec<LandlockRejectReason> {
        err.iter().map(|r| r.reason).collect()
    }

    #[test]
    fn accepts_explicit_tcp_port_allowlist_sorted_and_deduped() {
        let policy = NetworkPolicy {
            allow_ports: vec![443, 80, 443],
            ..net()
        };
        let target = compile_landlock_net(&policy).expect("explicit ports compile");
        assert_eq!(target.allowed_connect_tcp_ports, vec![80, 443]);
    }

    #[test]
    fn accepts_empty_allowlist_as_deny_all() {
        let target = compile_landlock_net(&net()).expect("empty allowlist is deny-all");
        assert!(target.allowed_connect_tcp_ports.is_empty());
    }

    #[test]
    fn rejects_allow_cidrs() {
        let policy = NetworkPolicy {
            allow_cidrs: vec!["10.0.0.0/8".to_string()],
            allow_ports: vec![443],
            ..net()
        };
        let err = compile_landlock_net(&policy).unwrap_err();
        assert_eq!(reasons(&err), vec![LandlockRejectReason::Cidr]);
    }

    #[test]
    fn rejects_deny_cidrs_including_ip_literal() {
        for cidr in ["203.0.113.10/32", "10.0.0.0/8"] {
            let policy = NetworkPolicy {
                deny_cidrs: vec![cidr.to_string()],
                ..net()
            };
            let err = compile_landlock_net(&policy).unwrap_err();
            assert_eq!(reasons(&err), vec![LandlockRejectReason::Cidr]);
        }
    }

    #[test]
    fn rejects_port_deny_as_negative_rule() {
        let policy = NetworkPolicy {
            deny_ports: vec![4444],
            ..net()
        };
        let err = compile_landlock_net(&policy).unwrap_err();
        assert_eq!(reasons(&err), vec![LandlockRejectReason::NegativeDeny]);
    }

    #[test]
    fn rejects_host_and_wildcard_destinations() {
        for dest in ["example.com:443", "*.internal:443", "10.0.0.1:80"] {
            let policy = NetworkPolicy {
                deny_destinations: vec![dest.to_string()],
                ..net()
            };
            let err = compile_landlock_net(&policy).unwrap_err();
            assert_eq!(reasons(&err), vec![LandlockRejectReason::Destination]);
        }
    }

    #[test]
    fn rejects_port_zero() {
        let policy = NetworkPolicy {
            allow_ports: vec![443, 0],
            ..net()
        };
        let err = compile_landlock_net(&policy).unwrap_err();
        assert_eq!(reasons(&err), vec![LandlockRejectReason::PortZero]);
    }

    #[test]
    fn collects_every_reason_not_just_the_first() {
        let policy = NetworkPolicy {
            allow_cidrs: vec!["10.0.0.0/8".to_string()],
            deny_ports: vec![4444],
            deny_destinations: vec!["example.com:443".to_string()],
            allow_ports: vec![0, 443],
            ..net()
        };
        let err = compile_landlock_net(&policy).unwrap_err();
        assert_eq!(
            reasons(&err),
            vec![
                LandlockRejectReason::Cidr,
                LandlockRejectReason::NegativeDeny,
                LandlockRejectReason::Destination,
                LandlockRejectReason::PortZero,
            ]
        );
    }

    #[test]
    fn reason_ids_are_stable_machine_readable_strings() {
        assert_eq!(LandlockRejectReason::Cidr.as_str(), "cidr_not_expressible");
        assert_eq!(
            LandlockRejectReason::NegativeDeny.as_str(),
            "negative_deny_not_expressible"
        );
        assert_eq!(
            LandlockRejectReason::Destination.as_str(),
            "destination_not_expressible"
        );
        assert_eq!(
            LandlockRejectReason::PortZero.as_str(),
            "port_zero_not_allowed"
        );
    }

    #[test]
    fn target_is_tcp_connect_port_typed_no_ranges_no_protocol() {
        // By construction: ports are u16 (no range syntax can be expressed) and there is no protocol
        // selector in the target (TCP-connect only). This guards the representability boundary.
        let target = compile_landlock_net(&NetworkPolicy {
            allow_ports: vec![443],
            ..net()
        })
        .unwrap();
        let _: Vec<u16> = target.allowed_connect_tcp_ports;
    }
}
