use super::types::JudgeMetrics;

/// Compute judge reliability metrics from run results (E7.3).
/// Returns None if no results have judge details.
/// One test can contribute multiple evaluations (one per metric name, e.g. faithfulness + relevance); rates are per-evaluation.
pub fn judge_metrics_from_results(results: &[crate::model::TestResultRow]) -> Option<JudgeMetrics> {
    use crate::model::TestStatus;

    let mut total_judge = 0u32;
    let mut abstain_count = 0u32;
    let mut consensus_count = 0u32;
    let mut flip_count = 0u32;

    for r in results {
        let Some(metrics) = r.details.get("metrics").and_then(|m| m.as_object()) else {
            continue;
        };
        for (_name, metric_val) in metrics {
            let Some(details) = metric_val.get("details") else {
                continue;
            };
            let verdict = details.get("verdict").and_then(|v| v.as_str());
            let agreement = details.get("agreement").and_then(|v| v.as_f64());
            let swapped = details
                .get("swapped")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if verdict.is_none() && agreement.is_none() {
                continue;
            }
            total_judge += 1;

            if verdict == Some("Abstain") {
                abstain_count += 1;
            }
            if let Some(a) = agreement {
                if a == 0.0 || a == 1.0 {
                    consensus_count += 1;
                }
                // flip_rate: heuristic proxy for "order was swapped and outcome differed".
                // We do not store the counterfactual verdict, so we use: swapped + non-unanimous
                // (0 < agreement < 1). This does NOT guarantee the verdict actually flipped;
                // it indicates order may have affected outcome. Strict definition would require
                // the judge to record whether pass/fail differed under the other ordering.
                if swapped && a > 0.0 && a < 1.0 {
                    flip_count += 1;
                }
            }
        }
    }

    if total_judge == 0 {
        return None;
    }

    let total = total_judge as f64;
    Some(JudgeMetrics {
        abstain_rate: Some(abstain_count as f64 / total),
        flip_rate: Some(flip_count as f64 / total),
        consensus_rate: Some(consensus_count as f64 / total),
        unavailable_count: Some(
            results
                .iter()
                .filter(|r| matches!(r.status, TestStatus::Error))
                .filter(|r| {
                    let m = r.message.to_lowercase();
                    m.contains("timeout")
                        || m.contains("500")
                        || m.contains("502")
                        || m.contains("503")
                        || m.contains("504")
                        || m.contains("rate limit")
                        || m.contains("network")
                })
                .count() as u32,
        ),
    })
}
