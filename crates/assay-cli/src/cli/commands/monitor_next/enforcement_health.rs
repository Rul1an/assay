//! `assay.enforcement_health.v0` — the enforcement-truth artifact.
//!
//! Deliberately a SEPARATE carrier from `observation_health`. observation_health answers "how complete
//! was observation?"; this answers "was enforcement active, and did it block?". They are orthogonal: a
//! run can have complete observation and absent enforcement, or vice versa. Conflating them would let an
//! observation artifact claim something it never produced.
//!
//! Modelled as enforcement truth, not monitor output: an *enforcement run* produces it. Today the only
//! producer is `assay monitor` (the connect4 egress path), but the schema is producer-agnostic so future
//! enforcement paths (landlock, argument constraints, tool-decision) emit the same shape.
//!
//! Truth carrier, not a log derivative: it is written as an explicit artifact (`--enforcement-health
//! <path>`), never parsed back out of stdout. stdout stays diagnostic; this file is the contract.
//!
//! v0 is intentionally small. Rule IDs, policy refs, timestamps, provenance, and enforcement receipts
//! are follow-ups, not v0.

use std::path::Path;

use serde::{Deserialize, Serialize};

pub const SCHEMA_V0: &str = "assay.enforcement_health.v0";

/// The enforcement domain this artifact reports on. For now the only producer is connect4 IPv4/TCP
/// egress. A second domain (e.g. landlock filesystem, argument constraints) is a new `scope` value, and
/// a multi-domain shape would be an explicit `v1` bump, never a silent reinterpretation of `v0`.
pub const SCOPE_IPV4_TCP_CONNECT: &str = "ipv4_tcp_connect";

/// The enforcement status. The carrier vocabulary is the full set; a given producer emits only the
/// states that can actually occur for it (the connect4 producer emits Active / Failed / Absent —
/// NotApplicable is reserved for scopes where the channel does not apply).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkEnforcement {
    /// Requested and installed (the program is attached and the deny rules are loaded).
    Active,
    /// Not requested (no network deny rules in the policy, or no policy).
    Absent,
    /// Requested but could NOT be installed (attach failed / no kernel support). Emitted on the
    /// fail-closed abort path BEFORE exit, so a requested-but-failed enforcement is never mistaken for
    /// an un-requested one.
    Failed,
    /// Network enforcement does not apply to this scope/producer (reserved; not emitted by connect4).
    NotApplicable,
}

/// How hard the enforcement point is to route around.
///
/// Adopted from `draft-schrock-ep-authorization-receipts-07` §9, value semantics included, together
/// with the rule that gives it its point: *"marketing or compliance claims MUST NOT state a stronger
/// class than deployed."*
///
/// The status field already says whether enforcement ran. This says what it is worth if it did. A
/// consumer reading `active` alone cannot tell a kernel hook the subject cannot reach past from a
/// userspace proxy it can simply decline to call, and those are different guarantees.
///
/// **Derived, never declared.** It is computed from the constructor that recorded the attach truth,
/// so a run cannot claim a class it did not deploy — the same reason the coverage descriptor is keyed
/// to the probe that supplies peers rather than to the one that merely sees connects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnforcementClass {
    /// No enforcement claim is made. Nothing attached, so nothing was refused.
    Basic = 0,
    /// An interception layer between the subject and the effect. Real protection against error and
    /// injection, and reachable past: a subject that calls the tool by another route is not gated.
    /// The Tier-2 MCP proxy is this.
    Standard = 1,
    /// The enforcement point sits where the subject cannot route around it without privilege it does
    /// not have. The Tier-1 kernel `cgroup_sock_addr` egress hook is this: it fires on the kernel's
    /// own connect path, so it is not evaded by choosing a different syscall entry the way a
    /// tracepoint would be.
    ///
    /// Bounded honestly, in Schrock's own terms: strong against any party that does not control the
    /// enforcement point itself. Root can detach the program, and escaping the cgroup needs
    /// privilege; neither is a bypass available to the subject being governed.
    Strong = 2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnforcementHealth {
    pub schema: String,
    pub scope: String,
    pub network_enforcement: NetworkEnforcement,
    pub attach_confirmed: bool,
    pub blocked_count: u64,
    pub allowed_count: u64,
    /// What the enforcement is worth, derived from whether it actually attached.
    pub enforcement_class: EnforcementClass,
}

impl EnforcementHealth {
    fn new(scope: &str, status: NetworkEnforcement, attach_confirmed: bool) -> Self {
        Self {
            schema: SCHEMA_V0.to_string(),
            scope: scope.to_string(),
            network_enforcement: status,
            attach_confirmed,
            blocked_count: 0,
            allowed_count: 0,
            // Derived from the attach truth this constructor was called with, never passed in. The
            // only scope this producer emits is the connect4 cgroup hook, so a confirmed attach is
            // Strong and everything else makes no enforcement claim at all. A producer for a
            // userspace path emits Standard from its own constructor; the field is producer-agnostic
            // like the rest of the schema.
            enforcement_class: if attach_confirmed {
                EnforcementClass::Strong
            } else {
                EnforcementClass::Basic
            },
        }
    }

    /// Enforcement was requested and installed; carries the observed block/allow counts.
    pub fn active(scope: &str, blocked_count: u64, allowed_count: u64) -> Self {
        Self {
            blocked_count,
            allowed_count,
            ..Self::new(scope, NetworkEnforcement::Active, true)
        }
    }

    /// Enforcement was requested but could not be installed (fail-closed abort).
    pub fn failed(scope: &str) -> Self {
        Self::new(scope, NetworkEnforcement::Failed, false)
    }

    /// Enforcement was not requested for this run.
    pub fn absent(scope: &str) -> Self {
        Self::new(scope, NetworkEnforcement::Absent, false)
    }

    pub fn write_to(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_confirmed_attach_earns_strong() {
        // Schrock's rule is that a record MUST NOT state a stronger class than deployed. The class is
        // derived from the constructor that recorded the attach, so there is no argument through
        // which a caller could overstate it.
        assert_eq!(
            EnforcementHealth::active(SCOPE_IPV4_TCP_CONNECT, 1, 1).enforcement_class,
            EnforcementClass::Strong
        );
        for h in [
            EnforcementHealth::failed(SCOPE_IPV4_TCP_CONNECT),
            EnforcementHealth::absent(SCOPE_IPV4_TCP_CONNECT),
        ] {
            assert_eq!(
                h.enforcement_class,
                EnforcementClass::Basic,
                "no attach, so no enforcement claim"
            );
        }
    }

    #[test]
    fn a_requested_but_failed_enforcement_claims_no_more_than_an_unrequested_one() {
        // Failed and absent differ in status, which is the field that carries that distinction. On
        // the bypassability axis they are identical: nothing attached, so nothing was refused.
        // Grading failed above absent would let a fail-closed abort read as partial protection.
        let failed = EnforcementHealth::failed(SCOPE_IPV4_TCP_CONNECT);
        let absent = EnforcementHealth::absent(SCOPE_IPV4_TCP_CONNECT);
        assert_eq!(failed.enforcement_class, absent.enforcement_class);
        assert_ne!(
            failed.network_enforcement, absent.network_enforcement,
            "the distinction lives in status, and must still be visible there"
        );
    }

    #[test]
    fn the_class_is_ordered_so_a_consumer_can_require_a_minimum() {
        assert!(EnforcementClass::Basic < EnforcementClass::Standard);
        assert!(EnforcementClass::Standard < EnforcementClass::Strong);
    }

    #[test]
    fn the_class_is_on_the_wire_and_snake_case() {
        let json = serde_json::to_string(&EnforcementHealth::active(SCOPE_IPV4_TCP_CONNECT, 2, 3))
            .unwrap();
        assert!(json.contains(r#""enforcement_class":"strong""#), "{json}");
        let json =
            serde_json::to_string(&EnforcementHealth::absent(SCOPE_IPV4_TCP_CONNECT)).unwrap();
        assert!(json.contains(r#""enforcement_class":"basic""#), "{json}");
    }

    #[test]
    fn attach_confirmed_and_the_class_can_never_disagree() {
        // They are two readings of one fact, so a divergence would mean the record contradicts
        // itself. Asserting it here is what stops a future edit from setting one without the other.
        for h in [
            EnforcementHealth::active(SCOPE_IPV4_TCP_CONNECT, 0, 0),
            EnforcementHealth::failed(SCOPE_IPV4_TCP_CONNECT),
            EnforcementHealth::absent(SCOPE_IPV4_TCP_CONNECT),
        ] {
            assert_eq!(
                h.attach_confirmed,
                h.enforcement_class == EnforcementClass::Strong,
                "attach_confirmed={} but class={:?}",
                h.attach_confirmed,
                h.enforcement_class
            );
        }
    }

    #[test]
    fn active_carries_counts_and_confirmed_attach() {
        let h = EnforcementHealth::active(SCOPE_IPV4_TCP_CONNECT, 1, 1);
        assert_eq!(h.network_enforcement, NetworkEnforcement::Active);
        assert!(h.attach_confirmed);
        assert_eq!(h.blocked_count, 1);
        assert_eq!(h.allowed_count, 1);
        assert_eq!(h.schema, SCHEMA_V0);
    }

    #[test]
    fn failed_is_distinct_from_absent() {
        // The whole point of the artifact: requested-but-failed must not look like not-requested.
        let failed = EnforcementHealth::failed(SCOPE_IPV4_TCP_CONNECT);
        let absent = EnforcementHealth::absent(SCOPE_IPV4_TCP_CONNECT);
        assert_eq!(failed.network_enforcement, NetworkEnforcement::Failed);
        assert!(!failed.attach_confirmed);
        assert_eq!(absent.network_enforcement, NetworkEnforcement::Absent);
        assert_ne!(failed.network_enforcement, absent.network_enforcement);
    }

    #[test]
    fn serializes_snake_case_status() {
        let json = serde_json::to_string(&EnforcementHealth::active(SCOPE_IPV4_TCP_CONNECT, 2, 3))
            .unwrap();
        assert!(json.contains("\"schema\":\"assay.enforcement_health.v0\""));
        assert!(json.contains("\"network_enforcement\":\"active\""));
        assert!(json.contains("\"scope\":\"ipv4_tcp_connect\""));
        assert!(json.contains("\"blocked_count\":2"));
        assert!(json.contains("\"allowed_count\":3"));
    }

    #[test]
    fn round_trips_through_json() {
        let h = EnforcementHealth::failed(SCOPE_IPV4_TCP_CONNECT);
        let back: EnforcementHealth =
            serde_json::from_str(&serde_json::to_string(&h).unwrap()).unwrap();
        assert_eq!(back.network_enforcement, NetworkEnforcement::Failed);
        assert!(!back.attach_confirmed);
    }

    // write_to failure modes use a directory-as-path and a missing parent: both are portable and
    // deterministic, unlike permission bits which differ per OS/CI. A requested artifact that cannot
    // be written must surface as an error so the caller can refuse exit 0 (a missing file would
    // otherwise be read as "not requested").
    #[test]
    fn write_to_fails_when_path_is_a_directory() {
        let h = EnforcementHealth::active(SCOPE_IPV4_TCP_CONNECT, 1, 1);
        assert!(h.write_to(&std::env::temp_dir()).is_err());
    }

    #[test]
    fn write_to_fails_when_parent_dir_is_missing() {
        let h = EnforcementHealth::active(SCOPE_IPV4_TCP_CONNECT, 1, 1);
        let path = std::env::temp_dir()
            .join(format!("assay-eh-missing-{}", std::process::id()))
            .join("nested")
            .join("enforcement_health.json");
        assert!(h.write_to(&path).is_err());
    }

    #[test]
    fn write_to_succeeds_and_round_trips_from_disk() {
        let h = EnforcementHealth::active(SCOPE_IPV4_TCP_CONNECT, 2, 3);
        let path = std::env::temp_dir().join(format!("assay-eh-{}.json", std::process::id()));
        h.write_to(&path).unwrap();
        let back: EnforcementHealth =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(back.network_enforcement, NetworkEnforcement::Active);
        assert_eq!(back.blocked_count, 2);
        assert_eq!(back.allowed_count, 3);
    }
}
