use super::super::heuristics::{self, HeuristicsConfig};
use super::ingest::Aggregated;
use super::model::{Entry, Meta, NetSection, Policy, Section};

pub fn generate_from_trace(
    name: &str,
    agg: &Aggregated,
    use_heuristics: bool,
    cfg: &HeuristicsConfig,
) -> Policy {
    let mut files = Section::default();
    let mut network = NetSection::default();
    let mut processes = Section::default();

    for (path, stats) in &agg.files {
        let risk = if use_heuristics {
            Some(heuristics::analyze_entropy(path, cfg))
        } else {
            None
        };
        let entry = make_entry_simple(path, stats.count, risk.as_ref());
        match risk.as_ref().map(|r| &r.level) {
            Some(heuristics::RiskLevel::DenyRecommended) => files.deny.push(path.clone()),
            Some(heuristics::RiskLevel::NeedsReview) => files.needs_review.push(entry),
            _ => files.allow.push(entry),
        }
    }

    for (dest, stats) in &agg.network {
        let risk = if use_heuristics {
            Some(heuristics::analyze_dest(dest, cfg))
        } else {
            None
        };
        let entry = make_entry_simple(dest, stats.count, risk.as_ref());
        match risk.as_ref().map(|r| &r.level) {
            Some(heuristics::RiskLevel::DenyRecommended) => {
                network.deny_destinations.push(dest.clone())
            }
            Some(heuristics::RiskLevel::NeedsReview) => network.needs_review.push(entry),
            _ => network.allow_destinations.push(entry),
        }
    }

    for (path, stats) in &agg.processes {
        let risk = if use_heuristics {
            Some(heuristics::analyze_entropy(path, cfg))
        } else {
            None
        };
        let entry = make_entry_simple(path, stats.count, risk.as_ref());
        match risk.as_ref().map(|r| &r.level) {
            Some(heuristics::RiskLevel::DenyRecommended) => processes.deny.push(path.clone()),
            Some(heuristics::RiskLevel::NeedsReview) => processes.needs_review.push(entry),
            _ => processes.allow.push(entry),
        }
    }

    Policy {
        _meta: Some(Meta {
            name: name.into(),
            generated_at: chrono::Utc::now().to_rfc3339(),
            profile_runs: None,
            min_stability: None,
            min_runs: None,
        }),
        files,
        network,
        processes,
    }
}

fn make_entry_simple(
    pattern: &str,
    count: u32,
    risk: Option<&heuristics::RiskAssessment>,
) -> Entry {
    match risk {
        Some(r) if r.level > heuristics::RiskLevel::Low => Entry::WithMeta {
            pattern: pattern.into(),
            count: Some(count),
            stability: None,
            runs_seen: None,
            risk: match r.level {
                heuristics::RiskLevel::Low => Some("low".into()),
                heuristics::RiskLevel::NeedsReview => Some("needs_review".into()),
                heuristics::RiskLevel::DenyRecommended => Some("deny_recommended".into()),
            },
            reasons: if r.reasons.is_empty() {
                None
            } else {
                Some(r.reasons.clone())
            },
        },
        _ if count > 1 => Entry::WithMeta {
            pattern: pattern.into(),
            count: Some(count),
            stability: None,
            runs_seen: None,
            risk: None,
            reasons: None,
        },
        _ => Entry::Simple(pattern.into()),
    }
}
