//! Network-egress enforcement INTENT compiles to Tier-1 kernel rules.
//!
//! This pins the half of the egress story that is real today: a policy's `deny_cidrs` / `deny_ports`
//! compile into the Tier-1 maps the kernel `connect4` hook reads (`CIDR_RULES_V4` with `ACTION_DENY`,
//! `DENY_PORTS`). It does NOT assert that a connect is blocked at runtime -- the cgroup attach that
//! would activate the compiled program is a separate concern. Keeping the boundary explicit is the
//! point: the rule compilation is verified here; runtime enforcement is measured separately.

use assay_policy::tiers::{compile, FilePolicy, NetworkPolicy, Policy, ProcessPolicy};

const ACTION_ALLOW: u8 = 1;
const ACTION_DENY: u8 = 2;

fn egress_policy() -> Policy {
    Policy {
        files: FilePolicy::default(),
        network: NetworkPolicy {
            allow_cidrs: vec!["10.0.0.0/8".to_string()],
            deny_cidrs: vec!["203.0.113.0/24".to_string()], // TEST-NET-3 exfil range
            deny_ports: vec![4444, 9001],
            ..Default::default()
        },
        processes: ProcessPolicy::default(),
    }
}

#[test]
fn deny_cidr_compiles_to_action_deny_entry() {
    let compiled = compile(&egress_policy());
    let entries = compiled.tier1.cidr_v4_entries();

    let deny = entries
        .iter()
        .find(|(_, addr, _)| *addr == [203, 0, 113, 0])
        .expect("deny CIDR must produce a CIDR_RULES_V4 entry");
    assert_eq!(deny.0, 24, "prefix length must be preserved");
    assert_eq!(deny.2, ACTION_DENY, "deny CIDR must carry ACTION_DENY");

    let allow = entries
        .iter()
        .find(|(_, addr, _)| *addr == [10, 0, 0, 0])
        .expect("allow CIDR must produce a CIDR_RULES_V4 entry");
    assert_eq!(allow.2, ACTION_ALLOW, "allow CIDR must carry ACTION_ALLOW");
}

#[test]
fn deny_ports_compile_to_port_entries() {
    let compiled = compile(&egress_policy());
    let ports: Vec<u16> = compiled
        .tier1
        .port_deny_entries()
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    assert!(ports.contains(&4444), "deny port 4444 must compile");
    assert!(ports.contains(&9001), "deny port 9001 must compile");
}

#[test]
fn empty_network_policy_produces_no_egress_rules() {
    // Honest negative: no declared egress rules => no deny entries (no phantom enforcement).
    let compiled = compile(&Policy {
        files: FilePolicy::default(),
        network: NetworkPolicy::default(),
        processes: ProcessPolicy::default(),
    });
    assert!(compiled
        .tier1
        .cidr_v4_entries()
        .iter()
        .all(|(_, _, action)| *action != ACTION_DENY));
    assert!(compiled.tier1.port_deny_entries().is_empty());
}
