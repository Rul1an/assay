//! `assay.monitor.observed_peers.v0` — what a run actually saw on the watched connect surface.
//!
//! Cross-platform carrier, Linux-gated emitter, the same split `enforcement_health` uses: the record
//! is plain data and only the eBPF that fills it is Linux-only. That keeps it testable on a developer
//! machine, which matters because the invariant it carries is the one a refutation stands on.

/// The peer endpoints a run actually observed, written beside `observation_health`.
///
/// This artifact exists so a refutation cannot be assembled from parts of different runs. A peer set
/// only means anything against the coverage descriptor of the run that produced it: peers from run A
/// checked against run B's coverage would let a well-covered run vouch for a blind one. Both are
/// written by the same `assay monitor` invocation, and the schema carries `run_id` so a consumer can
/// refuse a mismatched pair.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ObservedPeers {
    pub schema: String,
    pub run_id: String,
    /// Distinct `destination:port` endpoints seen on the watched connect surface, sorted. Empty is a
    /// real observation when the coverage descriptor says the surface was watched, and means nothing
    /// when it does not: that judgement belongs to the consumer, not here.
    pub peers: Vec<String>,
}

pub const OBSERVED_PEERS_SCHEMA: &str = "assay.monitor.observed_peers.v0";

impl ObservedPeers {
    pub fn new(run_id: &str, mut peers: Vec<String>) -> Self {
        peers.sort();
        peers.dedup();
        Self {
            schema: OBSERVED_PEERS_SCHEMA.to_string(),
            run_id: run_id.to_string(),
            peers,
        }
    }

    pub fn write_to(&self, path: &std::path::Path) -> std::io::Result<()> {
        std::fs::write(path, serde_json::to_string_pretty(self).unwrap_or_default())
    }
}

#[cfg(test)]
mod observed_peers_tests {
    use super::*;

    #[test]
    fn peers_are_sorted_and_deduped_so_the_artifact_is_deterministic() {
        let p = ObservedPeers::new(
            "run-1",
            vec![
                "b.example:443".into(),
                "a.example:443".into(),
                "b.example:443".into(),
            ],
        );
        assert_eq!(p.peers, ["a.example:443", "b.example:443"]);
    }

    #[test]
    fn an_empty_peer_set_is_still_a_record() {
        // The artifact must exist even with nothing in it. A missing file reads as "not requested";
        // an empty peer list reads as "watched and saw nothing", and only the second can support a
        // refutation when the coverage descriptor agrees.
        let p = ObservedPeers::new("run-1", vec![]);
        assert!(p.peers.is_empty());
        assert_eq!(p.schema, OBSERVED_PEERS_SCHEMA);
        assert_eq!(p.run_id, "run-1");
    }
}
