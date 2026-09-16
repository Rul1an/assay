//! #3036 (from #3020 F2): pin the JSON wire shape of `CidrRule.parsed`.
//!
//! `CidrRule.parsed: IpNet` is serialised under `#[derive(Serialize)]`
//! (`crates/assay-policy/src/tiers_next/types.rs`) via ipnet's `serde` feature,
//! and no test asserted the wire shape. Emission audit: no production code
//! serialises a `CidrRule` (or its parent `CompiledPolicy`/`Tier1Rules`) to
//! JSON -- the loader and the map builders read struct fields (`rule.parsed`,
//! `rule.rule_id`, `rule.cidr`) and project them into LPM entries
//! (`cidr_v4_entries`, pinned in `network_egress_compile.rs`) or the IPv4-only
//! enforcement check (`assay-monitor`'s `validate_network_enforcement_support`).
//! So there is no larger JSON surface a consumer reads to pin instead; this
//! test pins the emission shape itself (exact field set plus ipnet's string
//! rendering of `parsed`) for an IPv4 and an IPv6 rule.
//!
//! `CidrRule` is `Serialize`-only, so a true deserialisation round-trip would
//! need a `Deserialize` impl -- a production change, deliberately not made here.
//!
//! Recorded against ipnet 2.12.2: `parsed` renders as the CIDR display string.

use assay_policy::tiers::{compile, FilePolicy, NetworkPolicy, Policy, ProcessPolicy};
use serde_json::json;

fn shape_policy() -> Policy {
    Policy {
        files: FilePolicy::default(),
        network: NetworkPolicy {
            allow_cidrs: vec!["10.0.0.0/8".to_string(), "2001:db8::/32".to_string()],
            deny_cidrs: vec!["203.0.113.0/24".to_string()],
            ..Default::default()
        },
        processes: ProcessPolicy::default(),
    }
}

#[test]
fn cidr_rule_parsed_serialises_as_cidr_string_for_both_families() {
    let compiled = compile(&shape_policy());
    let rules = compiled
        .tier1
        .network_allow_cidrs
        .iter()
        .chain(compiled.tier1.network_deny_cidrs.iter());

    let mut saw_v4 = false;
    let mut saw_v6 = false;
    let mut count = 0;
    for rule in rules {
        count += 1;
        let wire = serde_json::to_value(rule).expect("CidrRule serialises");
        assert_eq!(
            wire,
            json!({
                "rule_id": rule.rule_id,
                "cidr": rule.cidr.clone(),
                "parsed": rule.cidr.clone(),
            }),
            "exact CidrRule wire shape for {}",
            rule.cidr
        );
        if rule.parsed.addr().is_ipv6() {
            saw_v6 = true;
        } else {
            saw_v4 = true;
        }
    }
    assert_eq!(count, 3, "shape policy must compile three CIDR rules");
    assert!(
        saw_v4 && saw_v6,
        "pin needs both families: v4={saw_v4} v6={saw_v6}"
    );
}
