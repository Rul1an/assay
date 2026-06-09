//! `assay.enforcement_health.v1` — the enforcement-truth carrier for the Landlock TCP-connect
//! port-allowlist domain.
//!
//! This is a SEPARATE, explicit version bump from `assay.enforcement_health.v0` (the connect4/eBPF
//! egress carrier in `cli::commands::monitor_next::enforcement_health`). v0 is left untouched and
//! remains valid; a consumer reads v0 and v1 additively. The two carriers are different shapes
//! because they report different enforcement domains: v0 carries connect4 attach/count truth, v1
//! carries the Landlock ruleset/`restrict_self`/real-block truth.
//!
//! Scope: this is the CARRIER and its committed fixtures only. No producer wires it up yet — the
//! Landlock sandbox enforcement that emits it is a later, gated step. The fixtures pin the exact
//! bytes a consumer (e.g. an external review tool) reconstructs against, the same producer-agnostic
//! discipline the rest of the project uses.
//!
//! Honesty rules baked into the shape:
//! - `status` is `active` or `failed` only. There is no `not_applicable`, and no `absent`: the
//!   presence of a v1 artifact means Landlock enforcement was requested. A run that does not request
//!   Landlock enforcement simply writes no v1 artifact (and a requested-but-unwritable artifact must
//!   be a hard error at the producer, the same rule v0 enforces).
//! - `probe` is always present and is `null` when no real-block probe was run, so a consumer can
//!   distinguish "schema knows about probe, none happened" from an older shape that lacked the field.
//!   `active` without a probe means the ruleset was applied (`no_new_privs` + `restrict_self`
//!   confirmed); `active` with a probe additionally means a denied connect was really blocked before
//!   the listener was reached.
//! - `failure.reason_code` is a machine-readable enum, never prose only.

use serde::{Deserialize, Serialize};

pub const SCHEMA_V1: &str = "assay.enforcement_health.v1";

/// The enforcement domain. Mechanism-accurate (not IPv4-specific): Landlock is not fundamentally
/// IPv4-only, so the transport lives under `probe`, not in the scope string.
pub const SCOPE_TCP_CONNECT_LANDLOCK_PORT: &str = "tcp_connect_landlock_port";

/// Whether enforcement was active or failed to install. No `not_applicable`; `absent` is modelled by
/// the artifact not existing (presence means requested).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// The Landlock ruleset was applied: `no_new_privs` set and `restrict_self` confirmed.
    Active,
    /// Enforcement was requested but could not be installed (carries `failure.reason_code`).
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mechanism {
    Landlock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicySemantics {
    /// Landlock network rules are allowlist-only: a handled right is denied by default, and rules
    /// grant exceptions for specific TCP ports. There is no endpoint/CIDR/negative-deny.
    Allowlist,
}

/// Machine-readable failure reasons. Only states that can actually occur on the Landlock TCP-connect
/// path (mirrors the PR2 usability-smoke failure taxonomy plus the ABI gate).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasonCode {
    /// Landlock ABI < 4: `LANDLOCK_ACCESS_NET_CONNECT_TCP` is unavailable.
    LandlockAbiTooOld,
    /// Landlock is built in but disabled at boot (`EOPNOTSUPP`).
    LandlockDisabled,
    /// `PR_SET_NO_NEW_PRIVS` could not be set, so an unprivileged `restrict_self` cannot apply.
    NoNewPrivsFailed,
    /// `landlock_restrict_self` returned an error or did not enforce.
    RestrictSelfFailed,
    /// The ruleset could not be built (handle-access or add-rule failed).
    RulesetBuildFailed,
    /// The network policy is not expressible as a Landlock TCP-connect allowlist (IP/CIDR, host,
    /// non-TCP protocol, port range, deny rule, or port 0). The specific entries are in `detail`.
    PolicyNotExpressible,
}

/// The Landlock-specific evidence block. Fields that do not apply to a given status are omitted
/// (`active` carries the applied-ruleset shape; `failed` carries the partial-capability shape), so
/// the serialized bytes match the domain exactly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LandlockBlock {
    /// The runtime Landlock ABI observed (may be < 4 on the failed-too-old path).
    pub abi: u32,
    /// Present on the failed path: whether ABI >= 4 (`CONNECT_TCP`) was available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_connect_tcp_supported: Option<bool>,
    /// Present on the active path: the handled network access rights (e.g. `["connect_tcp"]`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handled_access_net: Option<Vec<String>>,
    /// Present on the active path: the explicit allowed TCP-connect ports (allowlist, never a range).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_connect_tcp_ports: Option<Vec<u16>>,
    /// Whether `PR_SET_NO_NEW_PRIVS` was confirmed set before `restrict_self`. A
    /// `restrict_self_confirmed = true` without this is too weak for this route.
    pub no_new_privs_confirmed: bool,
    /// Whether `landlock_restrict_self` was confirmed (by the enforcing child, not assumed by parent).
    pub restrict_self_confirmed: bool,
}

/// The optional real-block probe. Its presence upgrades the claim from "ruleset applied" to "a denied
/// connect was really blocked before the listener was reached". `blocked_errno` is the
/// mechanism-specific signal (Landlock denies connect with `EACCES`); weak signals such as a timeout
/// or `ECONNREFUSED` never count as a block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Probe {
    pub kind: String,
    pub transport: String,
    pub blocked_action: String,
    pub blocked_port: u16,
    pub blocked_errno: String,
    pub listener_reached: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    pub reason_code: ReasonCode,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnforcementHealthV1 {
    pub schema: String,
    pub status: Status,
    pub mechanism: Mechanism,
    pub scope: String,
    pub policy_semantics: PolicySemantics,
    /// Present only on the failed path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<Failure>,
    pub landlock: LandlockBlock,
    /// Always serialized; `null` when no real-block probe ran (null-over-absent, by design).
    pub probe: Option<Probe>,
    pub non_claims: Vec<String>,
}

/// The standing non-claims for the Landlock TCP-connect port-allowlist domain.
fn landlock_non_claims() -> Vec<String> {
    [
        "no ip or cidr enforcement",
        "no hostname enforcement",
        "no destination identity enforcement",
        "no udp or quic enforcement",
        "no http or tls route policy",
        "not a replacement for cgroup/connect4 endpoint enforcement",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

impl EnforcementHealthV1 {
    /// Active Landlock TCP-connect enforcement: the ruleset was applied (`no_new_privs` set and
    /// `restrict_self` confirmed). `probe` is `Some` only when a real-block probe was actually run;
    /// `None` records "ruleset applied, no real-block claim".
    #[must_use]
    pub fn landlock_active(abi: u32, allowed_ports: Vec<u16>, probe: Option<Probe>) -> Self {
        Self {
            schema: SCHEMA_V1.to_string(),
            status: Status::Active,
            mechanism: Mechanism::Landlock,
            scope: SCOPE_TCP_CONNECT_LANDLOCK_PORT.to_string(),
            policy_semantics: PolicySemantics::Allowlist,
            failure: None,
            landlock: LandlockBlock {
                abi,
                net_connect_tcp_supported: None,
                handled_access_net: Some(vec!["connect_tcp".to_string()]),
                allowed_connect_tcp_ports: Some(allowed_ports),
                no_new_privs_confirmed: true,
                restrict_self_confirmed: true,
            },
            probe,
            non_claims: landlock_non_claims(),
        }
    }

    /// Failed Landlock TCP-connect enforcement: requested but not installed. Carries the
    /// machine-readable reason and the partial-capability truth.
    #[must_use]
    pub fn landlock_failed(
        abi: u32,
        reason_code: ReasonCode,
        detail: impl Into<String>,
        net_connect_tcp_supported: bool,
        no_new_privs_confirmed: bool,
    ) -> Self {
        Self {
            schema: SCHEMA_V1.to_string(),
            status: Status::Failed,
            mechanism: Mechanism::Landlock,
            scope: SCOPE_TCP_CONNECT_LANDLOCK_PORT.to_string(),
            policy_semantics: PolicySemantics::Allowlist,
            failure: Some(Failure {
                reason_code,
                detail: detail.into(),
            }),
            landlock: LandlockBlock {
                abi,
                net_connect_tcp_supported: Some(net_connect_tcp_supported),
                handled_access_net: None,
                allowed_connect_tcp_ports: None,
                no_new_privs_confirmed,
                restrict_self_confirmed: false,
            },
            probe: None,
            non_claims: landlock_non_claims(),
        }
    }

    pub fn write_to(&self, path: &std::path::Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }
}

#[cfg(test)]
mod fixture_values {
    //! The canonical example values the committed fixtures are generated from. Kept next to the
    //! carrier so the fixtures and the typed shape can never drift: `emit_fixtures` writes these to
    //! disk, and the round-trip tests read them back and assert byte-stability.
    use super::*;

    fn base_non_claims() -> Vec<String> {
        [
            "no ip or cidr enforcement",
            "no hostname enforcement",
            "no destination identity enforcement",
            "no udp or quic enforcement",
            "no http or tls route policy",
            "not a replacement for cgroup/connect4 endpoint enforcement",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    pub fn active_with_probe() -> EnforcementHealthV1 {
        EnforcementHealthV1 {
            schema: SCHEMA_V1.to_string(),
            status: Status::Active,
            mechanism: Mechanism::Landlock,
            scope: SCOPE_TCP_CONNECT_LANDLOCK_PORT.to_string(),
            policy_semantics: PolicySemantics::Allowlist,
            failure: None,
            landlock: LandlockBlock {
                abi: 4,
                net_connect_tcp_supported: None,
                handled_access_net: Some(vec!["connect_tcp".to_string()]),
                allowed_connect_tcp_ports: Some(vec![443]),
                no_new_privs_confirmed: true,
                restrict_self_confirmed: true,
            },
            probe: Some(Probe {
                kind: "real_block".to_string(),
                transport: "ipv4".to_string(),
                blocked_action: "tcp_connect".to_string(),
                blocked_port: 4444,
                blocked_errno: "EACCES".to_string(),
                listener_reached: false,
            }),
            non_claims: base_non_claims(),
        }
    }

    pub fn active_no_probe() -> EnforcementHealthV1 {
        EnforcementHealthV1 {
            probe: None,
            ..active_with_probe()
        }
    }

    pub fn failed() -> EnforcementHealthV1 {
        EnforcementHealthV1 {
            schema: SCHEMA_V1.to_string(),
            status: Status::Failed,
            mechanism: Mechanism::Landlock,
            scope: SCOPE_TCP_CONNECT_LANDLOCK_PORT.to_string(),
            policy_semantics: PolicySemantics::Allowlist,
            failure: Some(Failure {
                reason_code: ReasonCode::LandlockAbiTooOld,
                detail: "Landlock ABI 4 is required for TCP connect port allowlists".to_string(),
            }),
            landlock: LandlockBlock {
                abi: 3,
                net_connect_tcp_supported: Some(false),
                handled_access_net: None,
                allowed_connect_tcp_ports: None,
                no_new_privs_confirmed: false,
                restrict_self_confirmed: false,
            },
            probe: None,
            non_claims: base_non_claims(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const FIXTURE_DIR: &str = "tests/fixtures/enforcement_health/v1";

    fn fixture_path(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(FIXTURE_DIR)
            .join(name)
    }

    /// Regenerates the committed fixtures from the canonical typed values. Run explicitly:
    /// `cargo test -p assay-cli --bin assay enforcement_health_v1::tests::emit_fixtures -- --ignored`.
    /// Not part of the normal run; the round-trip tests below are the guard that the committed bytes
    /// match the typed shape.
    #[test]
    #[ignore = "generator; run with --ignored to regenerate fixtures"]
    fn emit_fixtures() {
        for (name, value) in [
            (
                "active_with_probe.json",
                fixture_values::active_with_probe(),
            ),
            ("active_no_probe.json", fixture_values::active_no_probe()),
            ("failed.json", fixture_values::failed()),
        ] {
            let mut json = serde_json::to_string_pretty(&value).unwrap();
            json.push('\n');
            std::fs::write(fixture_path(name), json).unwrap();
        }
    }

    fn load_fixture(name: &str) -> (String, EnforcementHealthV1) {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(FIXTURE_DIR)
            .join(name);
        let bytes = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()));
        let value: EnforcementHealthV1 = serde_json::from_str(&bytes)
            .unwrap_or_else(|e| panic!("parse fixture {}: {e}", path.display()));
        (bytes, value)
    }

    /// Each committed fixture parses into the typed carrier and re-serializes to byte-identical JSON.
    /// This pins the exact contract bytes a second implementation reconstructs against.
    fn assert_round_trip(name: &str) -> EnforcementHealthV1 {
        let (bytes, value) = load_fixture(name);
        let reserialized = serde_json::to_string_pretty(&value).unwrap();
        assert_eq!(
            reserialized.trim_end(),
            bytes.trim_end(),
            "fixture {name} is not byte-stable through the typed carrier"
        );
        value
    }

    #[test]
    fn active_with_probe_round_trips_and_claims_real_block() {
        let v = assert_round_trip("active_with_probe.json");
        assert_eq!(v.schema, SCHEMA_V1);
        assert_eq!(v.status, Status::Active);
        assert!(v.failure.is_none());
        let probe = v.probe.expect("active_with_probe carries a probe");
        assert_eq!(probe.blocked_errno, "EACCES");
        assert!(
            !probe.listener_reached,
            "a real block must not reach the listener"
        );
        assert!(v.landlock.no_new_privs_confirmed);
        assert!(v.landlock.restrict_self_confirmed);
    }

    #[test]
    fn active_no_probe_round_trips_with_null_probe() {
        let v = assert_round_trip("active_no_probe.json");
        assert_eq!(v.status, Status::Active);
        assert!(
            v.probe.is_none(),
            "active without a probe carries probe: null (ruleset applied, no real-block claim)"
        );
        assert!(v.landlock.restrict_self_confirmed);
    }

    #[test]
    fn failed_round_trips_with_machine_readable_reason() {
        let v = assert_round_trip("failed.json");
        assert_eq!(v.status, Status::Failed);
        let failure = v.failure.expect("failed carries a failure block");
        assert_eq!(failure.reason_code, ReasonCode::LandlockAbiTooOld);
        assert!(!v.landlock.restrict_self_confirmed);
        assert_eq!(v.landlock.net_connect_tcp_supported, Some(false));
    }

    #[test]
    fn v1_status_has_no_not_applicable_or_absent() {
        // The carrier vocabulary is intentionally only active/failed; absent is "no artifact",
        // and not_applicable does not exist in v1.
        assert!(serde_json::from_str::<Status>("\"active\"").is_ok());
        assert!(serde_json::from_str::<Status>("\"failed\"").is_ok());
        assert!(serde_json::from_str::<Status>("\"absent\"").is_err());
        assert!(serde_json::from_str::<Status>("\"not_applicable\"").is_err());
    }

    #[test]
    fn schema_id_is_an_explicit_version_bump_from_v0() {
        assert_eq!(SCHEMA_V1, "assay.enforcement_health.v1");
        assert_ne!(
            SCHEMA_V1, "assay.enforcement_health.v0",
            "v1 must be a distinct schema id from v0 (explicit version bump, not a silent reshape)"
        );
    }
}
