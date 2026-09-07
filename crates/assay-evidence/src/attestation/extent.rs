//! Optional artifact-derived extent (ADR-049). Ordinary verification never allocates this state.
use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

use crate::types::EvidenceEvent;

const MAX_COUNT: u64 = 9_007_199_254_740_991;
const MAX_KEY_BYTES: usize = 1024 * 1024;
const PROFILE: &str = "assay.profile.finished";
const SANDBOX: &str = "assay.sandbox.summary";

/// Recomputed retained-event counts and explicitly qualified summary assertions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EvidenceExtent {
    retained_events_by_type: BTreeMap<String, u64>,
    observed: Observed,
}

impl EvidenceExtent {
    /// Exact retained event types, in sorted order; missing types are not synthesized as zero.
    pub fn retained_events_by_type(&self) -> &BTreeMap<String, u64> {
        &self.retained_events_by_type
    }

    /// Producer-reported summary assertions, or explicitly not stated.
    pub fn observed(&self) -> &Observed {
        &self.observed
    }
}

/// Qualification of the recognized summary; not independent host observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Observed {
    basis: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    counts: Option<ObservedCounts>,
}

impl Observed {
    /// Either `producer_reported` or `not_stated`.
    pub fn basis(&self) -> &str {
        self.basis
    }
    /// Selected summary type, if any.
    pub fn source_type(&self) -> Option<&str> {
        self.source_type
    }
    /// Counts read from the selected producer-authored summary.
    pub fn counts(&self) -> Option<&ObservedCounts> {
        self.counts.as_ref()
    }
}

/// Entry cardinalities asserted by a summary. Missing network means not stated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ObservedCounts {
    files: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    network: Option<u64>,
    processes: u64,
    sandbox_degradations: u64,
}

impl ObservedCounts {
    /// Reported file-entry cardinality.
    pub fn files(&self) -> u64 {
        self.files
    }
    /// Reported network-entry cardinality, absent for sandbox summaries.
    pub fn network(&self) -> Option<u64> {
        self.network
    }
    /// Reported program-entry cardinality, not hit counts.
    pub fn processes(&self) -> u64 {
        self.processes
    }
    /// Reported degradation-vector length.
    pub fn sandbox_degradations(&self) -> u64 {
        self.sandbox_degradations
    }
}

// One numeric conversion rule for both source families, claimed counts and histogram additions.
fn count(value: &Value, field: &'static str) -> Result<u64> {
    value
        .as_u64()
        .filter(|n| *n <= MAX_COUNT)
        .with_context(|| format!("invalid {field}: expected integer in 0..2^53-1"))
}

#[derive(Default)]
struct Histogram {
    counts: BTreeMap<String, u64>,
    key_bytes: usize,
}

impl Histogram {
    fn add(&mut self, key: &str, amount: u64) -> Result<()> {
        let previous = self.counts.get(key).copied();
        let next = previous
            .unwrap_or(0)
            .checked_add(amount)
            .context("extent.retained_events_by_type count overflow")?;
        let next = count(&Value::from(next), "extent.retained_events_by_type")?;
        if previous.is_none() {
            if self.counts.len() >= crate::json_strict::MAX_KEYS_PER_OBJECT {
                bail!("extent.retained_events_by_type key count limit");
            }
            let bytes = self
                .key_bytes
                .checked_add(key.len())
                .context("extent.retained_events_by_type key bytes overflow")?;
            if bytes > MAX_KEY_BYTES {
                bail!("extent.retained_events_by_type key bytes limit");
            }
            // All bounds precede the new-key allocation and insertion.
            self.counts.insert(key.to_owned(), next);
            self.key_bytes = bytes;
        } else {
            *self.counts.get_mut(key).expect("existing histogram key") = next;
        }
        Ok(())
    }
}

fn source_counts(payload: &Value, profile: bool) -> Result<ObservedCounts> {
    let (files, processes, degradations) = if profile {
        (
            "files_count",
            "processes_count",
            "sandbox_degradation_count",
        )
    } else {
        ("fs_count", "exec_count", "degradation_count")
    };
    Ok(ObservedCounts {
        files: count(&payload[files], "extent.observed.counts.files")?,
        network: if profile {
            Some(count(
                &payload["network_count"],
                "extent.observed.counts.network",
            )?)
        } else {
            None
        },
        processes: count(&payload[processes], "extent.observed.counts.processes")?,
        sandbox_degradations: count(
            &payload[degradations],
            "extent.observed.counts.sandbox_degradations",
        )?,
    })
}

/// Collected only within the existing canonical verification pass.
#[derive(Default)]
pub(crate) struct Collector {
    histogram: Histogram,
    profile: Option<ObservedCounts>,
    sandbox: Option<ObservedCounts>,
}

impl Collector {
    pub(crate) fn observe(&mut self, event: &EvidenceEvent) -> Result<()> {
        self.histogram.add(&event.type_, 1)?;
        let slot = match event.type_.as_str() {
            PROFILE => &mut self.profile,
            SANDBOX => &mut self.sandbox,
            _ => return Ok(()),
        };
        if slot.is_some() {
            bail!("extent.observed repeated recognized summary");
        }
        // Even an unselected lower-ranked summary is validated.
        *slot = Some(source_counts(&event.payload, event.type_ == PROFILE)?);
        Ok(())
    }

    pub(crate) fn finish(self) -> EvidenceExtent {
        let selected = self
            .profile
            .map(|counts| (PROFILE, counts))
            .or_else(|| self.sandbox.map(|counts| (SANDBOX, counts)));
        let observed = match selected {
            Some((source_type, counts)) => Observed {
                basis: "producer_reported",
                source_type: Some(source_type),
                counts: Some(counts),
            },
            None => Observed {
                basis: "not_stated",
                source_type: None,
                counts: None,
            },
        };
        EvidenceExtent {
            retained_events_by_type: self.histogram.counts,
            observed,
        }
    }
}

/// Parse only recognized members; histogram keys remain data, never ignored extensions.
pub(crate) fn parse(value: &Value) -> Result<EvidenceExtent> {
    let object = value
        .as_object()
        .context("invalid extent: expected object")?;
    let entries = object
        .get("retained_events_by_type")
        .and_then(Value::as_object)
        .context("invalid extent.retained_events_by_type")?;
    let mut histogram = Histogram::default();
    for (key, value) in entries {
        histogram.add(key, count(value, "extent.retained_events_by_type")?)?;
    }
    let observed = object
        .get("observed")
        .and_then(Value::as_object)
        .context("invalid extent.observed")?;
    let observed = match observed.get("basis").and_then(Value::as_str) {
        Some("not_stated") => {
            if observed.contains_key("source_type") || observed.contains_key("counts") {
                bail!("invalid extent.observed: not_stated contradicts source_type or counts");
            }
            Observed {
                basis: "not_stated",
                source_type: None,
                counts: None,
            }
        }
        Some("producer_reported") => {
            let source_type = match observed.get("source_type").and_then(Value::as_str) {
                Some(PROFILE) => PROFILE,
                Some(SANDBOX) => SANDBOX,
                _ => bail!("invalid extent.observed.source_type"),
            };
            let counts = observed
                .get("counts")
                .filter(|v| v.is_object())
                .context("invalid extent.observed.counts")?;
            if source_type == SANDBOX && counts.get("network").is_some() {
                bail!("invalid extent.observed.counts.network: sandbox does not state network");
            }
            let counts = ObservedCounts {
                files: count(&counts["files"], "extent.observed.counts.files")?,
                network: if source_type == PROFILE {
                    Some(count(&counts["network"], "extent.observed.counts.network")?)
                } else {
                    None
                },
                processes: count(&counts["processes"], "extent.observed.counts.processes")?,
                sandbox_degradations: count(
                    &counts["sandbox_degradations"],
                    "extent.observed.counts.sandbox_degradations",
                )?,
            };
            Observed {
                basis: "producer_reported",
                source_type: Some(source_type),
                counts: Some(counts),
            }
        }
        _ => bail!("invalid extent.observed.basis"),
    };
    Ok(EvidenceExtent {
        retained_events_by_type: histogram.counts,
        observed,
    })
}

pub(crate) fn check(attested: &EvidenceExtent, derived: &EvidenceExtent) -> Result<()> {
    // Exhaustive destructures bind the mismatch inventory to the private type layouts.
    let EvidenceExtent {
        retained_events_by_type: a,
        observed: ao,
    } = attested;
    let EvidenceExtent {
        retained_events_by_type: d,
        observed: do_,
    } = derived;
    let Observed {
        basis: ab,
        source_type: ast,
        counts: ac,
    } = ao;
    let Observed {
        basis: db,
        source_type: dst,
        counts: dc,
    } = do_;
    let mismatch = if a != d {
        Some("extent.retained_events_by_type")
    } else if ab != db {
        Some("extent.observed.basis")
    } else if ast != dst {
        Some("extent.observed.source_type")
    } else {
        match (ac, dc) {
            (Some(a), Some(d)) => {
                let ObservedCounts {
                    files: af,
                    network: an,
                    processes: ap,
                    sandbox_degradations: ad,
                } = a;
                let ObservedCounts {
                    files: df,
                    network: dn,
                    processes: dp,
                    sandbox_degradations: dd,
                } = d;
                if af != df {
                    Some("extent.observed.counts.files")
                } else if an != dn {
                    Some("extent.observed.counts.network")
                } else if ap != dp {
                    Some("extent.observed.counts.processes")
                } else if ad != dd {
                    Some("extent.observed.counts.sandbox_degradations")
                } else {
                    None
                }
            }
            (None, None) => None,
            _ => Some("extent.observed.counts"),
        }
    };
    if let Some(field) = mismatch {
        bail!("predicate field `{field}` disagrees with the bundle it is attached to");
    }
    Ok(())
}
