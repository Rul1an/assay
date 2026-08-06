//! EXPERIMENTAL (Ec): refuting an effect claim from below the harness.
//!
//! Eb asks whether an independently produced record *corroborates* an effect. This asks the opposite
//! question, which nobody else in the field can ask, and only because R1 and R2 are already in place:
//!
//! > The record says this call went out. A first-party kernel observer says nothing left this cgroup
//! > on a surface it was watching. Those disagree, and the coverage descriptor proves it was watching.
//!
//! # Why this needs a denominator, and why it is refused without one
//!
//! "We saw no connect" is worth nothing on its own. An observer that was not watching, whose probe
//! never attached, whose ring buffer dropped records, or whose cgroup correlation was partial
//! produces exactly the same silence as one that watched and genuinely saw nothing. Treating those
//! alike is the false-GREEN this lab caught its own settlement probe committing.
//!
//! So refutation is gated on positive coverage, and every way of lacking it is a distinct outcome
//! that says which. A run cannot refute by being blind.
//!
//! # What a refutation is and is not
//!
//! It says a claimed egress did not leave on a watched surface. It does not say the effect failed:
//! a call that did leave may still have achieved nothing, and one that never left may have been
//! carried by a path outside the probe set (`io_uring`, a helper process, a surface we do not watch).
//! Those are the reasons `refuted` names the surface it watched rather than the world.

use serde::Serialize;
use serde_json::Value;

/// What a below-harness observer can conclude about a claimed egress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum EgressRefutation {
    /// Watched the surface, saw a matching connect. Supports the occurrence.
    Corroborated { peer: String },
    /// Watched the surface with clean correlation and no drops, and saw nothing. The record and the
    /// kernel disagree.
    Refuted { watched_surface: &'static str },
    /// The dimension was not observed at all, so silence is silence.
    NoCoverage { reason: &'static str },
    /// Observed, but the run cannot account for everything it saw or missed, so absence is not
    /// establishable. Degraded rather than silent, per the plan's rule for Partial/Failed.
    CoverageDegraded { reason: &'static str },
}

impl EgressRefutation {
    #[must_use]
    pub fn refutes(&self) -> bool {
        matches!(self, Self::Refuted { .. })
    }
}

/// The probe that must be attached before an egress claim can be refuted.
const EGRESS_PROBE_SURFACE: &str = "cgroup_sock_addr:connect4";

/// Decide what a below-harness observer can say about a claimed egress.
///
/// `observation_health` is an `assay.runner.observation_health.v0` value, read as plain JSON exactly
/// as `project_otel` consumes it — no crate edge is added for this. `observed_peers` is what the
/// observer actually recorded for the correlated cgroup.
///
/// The order of the guards is the argument. Every reason the observer might be blind is checked
/// **before** absence is read as evidence, because the whole failure mode being prevented is a blind
/// run reporting a clean result.
#[must_use]
pub fn refute_egress(
    observation_health: Option<&Value>,
    observed_peers: &[String],
    expected_peer: Option<&str>,
) -> EgressRefutation {
    let Some(oh) = observation_health else {
        return EgressRefutation::NoCoverage {
            reason: "no observation_health artifact: nothing declares what was watched",
        };
    };

    // 1. Did the kernel layer run at all?
    match oh.get("kernel_layer").and_then(Value::as_str) {
        Some("complete") => {}
        Some("partial_ringbuf_drops") => {
            return EgressRefutation::CoverageDegraded {
                reason: "kernel layer lost records to ring-buffer pressure; a missing connect may \
                         have been dropped rather than absent",
            }
        }
        _ => {
            return EgressRefutation::NoCoverage {
                reason: "kernel layer absent: nothing was watching",
            }
        }
    }

    // 2. Even a complete layer that dropped records cannot support an absence claim.
    if oh.get("ringbuf_drops").and_then(Value::as_u64).unwrap_or(0) > 0 {
        return EgressRefutation::CoverageDegraded {
            reason: "ring-buffer drops recorded: the observer knows it missed events",
        };
    }

    // 3. Was the network dimension covered, and to what depth?
    match oh
        .get("network_protocol_coverage")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
    {
        "connect_only" | "connect_and_datagram_peer_observed" => {}
        "datagram_peer_observed" => {
            return EgressRefutation::CoverageDegraded {
                reason:
                    "datagram peers observed but connect was not: a connect-based egress could \
                         have occurred unseen",
            }
        }
        "absent" => {
            return EgressRefutation::NoCoverage {
                reason: "network coverage absent: no egress probe attached",
            }
        }
        _ => {
            return EgressRefutation::NoCoverage {
                reason: "network coverage unknown: the run does not declare whether it watched",
            }
        }
    }

    // 4. Can the observation be tied to this workload at all? A partial correlation means events
    //    exist that we cannot attribute, so their absence here proves nothing.
    match oh
        .get("cgroup_correlation")
        .and_then(Value::as_str)
        .unwrap_or("failed")
    {
        "clean" => {}
        "partial" => {
            return EgressRefutation::CoverageDegraded {
                reason:
                    "cgroup correlation partial: an egress may have occurred under a cgroup the \
                         run could not attribute",
            }
        }
        _ => {
            return EgressRefutation::NoCoverage {
                reason: "cgroup correlation failed: observations cannot be tied to this workload",
            }
        }
    }

    // Only here has the run earned the right to read silence as evidence.
    match expected_peer {
        Some(peer) if observed_peers.iter().any(|p| p == peer) => EgressRefutation::Corroborated {
            peer: peer.to_string(),
        },
        _ => EgressRefutation::Refuted {
            watched_surface: EGRESS_PROBE_SURFACE,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A run that watched properly and can account for what it saw.
    fn clean_health() -> Value {
        json!({
            "schema": "assay.runner.observation_health.v0",
            "kernel_layer": "complete",
            "ringbuf_drops": 0,
            "network_protocol_coverage": "connect_only",
            "cgroup_correlation": "clean",
        })
    }

    #[test]
    fn a_watched_run_that_saw_nothing_refutes_the_claim() {
        let r = refute_egress(Some(&clean_health()), &[], Some("api.github.com:443"));
        assert!(r.refutes(), "{r:?}");
        match r {
            EgressRefutation::Refuted { watched_surface } => {
                assert_eq!(
                    watched_surface, EGRESS_PROBE_SURFACE,
                    "a refutation names its surface"
                )
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_watched_run_that_saw_the_connect_corroborates() {
        let peers = vec!["api.github.com:443".to_string()];
        let r = refute_egress(Some(&clean_health()), &peers, Some("api.github.com:443"));
        assert!(matches!(r, EgressRefutation::Corroborated { .. }), "{r:?}");
    }

    // ---- every way of being blind must refuse to refute ----

    #[test]
    fn no_artifact_cannot_refute() {
        let r = refute_egress(None, &[], Some("api.github.com:443"));
        assert!(!r.refutes());
        assert!(matches!(r, EgressRefutation::NoCoverage { .. }), "{r:?}");
    }

    #[test]
    fn ringbuf_drops_cannot_refute() {
        // The observer knows it missed events. A missing connect may be one of them.
        let mut h = clean_health();
        h["ringbuf_drops"] = json!(3);
        let r = refute_egress(Some(&h), &[], Some("api.github.com:443"));
        assert!(!r.refutes());
        assert!(
            matches!(r, EgressRefutation::CoverageDegraded { .. }),
            "{r:?}"
        );
    }

    #[test]
    fn an_unattached_network_probe_cannot_refute() {
        let mut h = clean_health();
        h["network_protocol_coverage"] = json!("absent");
        let r = refute_egress(Some(&h), &[], Some("api.github.com:443"));
        assert!(!r.refutes());
        assert!(matches!(r, EgressRefutation::NoCoverage { .. }), "{r:?}");
    }

    #[test]
    fn unknown_coverage_cannot_refute() {
        // The default state of the schema. A run that does not declare what it watched has not
        // earned an absence claim, which is the rule that keeps `unknown` from reading as clean.
        let mut h = clean_health();
        h["network_protocol_coverage"] = json!("unknown");
        let r = refute_egress(Some(&h), &[], Some("api.github.com:443"));
        assert!(!r.refutes());
    }

    #[test]
    fn partial_cgroup_correlation_cannot_refute() {
        // The plan's kill criterion: if Partial cannot be distinguished from Clean, the refutation
        // has no denominator and must not ship. It is distinguished here, and it degrades.
        let mut h = clean_health();
        h["cgroup_correlation"] = json!("partial");
        let r = refute_egress(Some(&h), &[], Some("api.github.com:443"));
        assert!(!r.refutes());
        assert!(
            matches!(r, EgressRefutation::CoverageDegraded { .. }),
            "{r:?}"
        );
    }

    #[test]
    fn failed_cgroup_correlation_cannot_refute() {
        let mut h = clean_health();
        h["cgroup_correlation"] = json!("failed");
        let r = refute_egress(Some(&h), &[], Some("api.github.com:443"));
        assert!(!r.refutes());
        assert!(matches!(r, EgressRefutation::NoCoverage { .. }), "{r:?}");
    }

    #[test]
    fn a_degraded_kernel_layer_cannot_refute() {
        let mut h = clean_health();
        h["kernel_layer"] = json!("partial_ringbuf_drops");
        let r = refute_egress(Some(&h), &[], Some("api.github.com:443"));
        assert!(!r.refutes());
        assert!(
            matches!(r, EgressRefutation::CoverageDegraded { .. }),
            "{r:?}"
        );
    }

    #[test]
    fn datagram_only_coverage_cannot_refute_a_connect() {
        // Watching sendto/sendmsg says nothing about whether a connect happened.
        let mut h = clean_health();
        h["network_protocol_coverage"] = json!("datagram_peer_observed");
        let r = refute_egress(Some(&h), &[], Some("api.github.com:443"));
        assert!(!r.refutes());
    }

    #[test]
    fn the_coverage_guard_is_what_produces_the_refusal_not_the_empty_peer_set() {
        // Mutation check. Every negative case above passes an EMPTY observed-peer set, exactly like
        // the refuting case, so the tests would still pass if the guards did nothing and the code
        // simply never refuted. Flip only the coverage — same empty peers — and the verdict must
        // move. If it does not, the denominator is decorative.
        let blind = json!({
            "kernel_layer": "complete", "ringbuf_drops": 0,
            "network_protocol_coverage": "absent", "cgroup_correlation": "clean",
        });
        assert!(!refute_egress(Some(&blind), &[], Some("peer")).refutes());

        let mut watching = blind.clone();
        watching["network_protocol_coverage"] = json!("connect_only");
        assert!(
            refute_egress(Some(&watching), &[], Some("peer")).refutes(),
            "same empty peer set, only coverage changed: the guard must be what decides"
        );
    }

    #[test]
    fn every_blind_state_is_distinguishable_from_a_clean_refutation() {
        // The property the whole module exists for, asserted as one statement: no way of being
        // unable to see produces the same answer as having seen nothing.
        let blind = [
            json!({"kernel_layer": "absent"}),
            json!({"kernel_layer": "complete", "ringbuf_drops": 1, "network_protocol_coverage": "connect_only", "cgroup_correlation": "clean"}),
            json!({"kernel_layer": "complete", "ringbuf_drops": 0, "network_protocol_coverage": "absent", "cgroup_correlation": "clean"}),
            json!({"kernel_layer": "complete", "ringbuf_drops": 0, "network_protocol_coverage": "connect_only", "cgroup_correlation": "partial"}),
        ];
        for h in blind {
            let r = refute_egress(Some(&h), &[], Some("peer"));
            assert!(!r.refutes(), "blind run must not refute: {h} -> {r:?}");
        }
        assert!(refute_egress(Some(&clean_health()), &[], Some("peer")).refutes());
    }
}
