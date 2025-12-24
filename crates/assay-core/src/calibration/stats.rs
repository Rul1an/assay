use super::model::{CalibrationReport, MetricKey, MetricSummary};
use crate::model::TestResultRow;
use std::collections::HashMap;

pub struct Aggregator {
    target_tail: f64,
    // Store all raw values for each metric key to compute exact percentiles
    values: HashMap<MetricKey, Vec<f64>>,
}

impl Aggregator {
    pub fn new(target_tail: f64) -> Self {
        Self {
            target_tail,
            values: HashMap::new(),
        }
    }

    pub fn push(&mut self, key: MetricKey, v: f64) {
        self.values.entry(key).or_default().push(v);
    }

    pub fn finish(self, source: &str) -> CalibrationReport {
        let mut metrics = Vec::new();

        for (key, mut vs) in self.values {
            // Sort for percentile calculation
            vs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

            let n = vs.len() as u32;
            if n == 0 {
                continue;
            }

            let min = *vs.first().unwrap();
            let max = *vs.last().unwrap();

            let sum: f64 = vs.iter().sum();
            let mean = sum / (n as f64);

            let variance: f64 = vs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n as f64);
            let std = variance.sqrt();

            let p10 = percentile(&vs, 0.10);
            let p50 = percentile(&vs, 0.50);
            let p90 = percentile(&vs, 0.90);

            // Recommendation Logic
            // 1. Min Score: Use the target tail (e.g. p10) as the safe floor
            let recommended_min_score = percentile(&vs, self.target_tail);

            // 2. Max Drop: How much variance is normal? (Median - Tail)
            // Clamp it between 2% (noise floor) and 10% (safety ceiling)
            let recommended_max_drop = (p50 - p10).clamp(0.02, 0.10);

            metrics.push(MetricSummary {
                key,
                n,
                min,
                max,
                mean,
                std,
                p10,
                p50,
                p90,
                recommended_min_score,
                recommended_max_drop,
            });
        }

        // Deterministic output order
        metrics.sort_by(|a, b| a.key.metric.cmp(&b.key.metric));

        let mut notes = vec![];
        if metrics.iter().any(|m| m.n < 10) {
            notes.push(
                "Warning: Low sample size (n < 10) makes percentiles unreliable.".to_string(),
            );
        }

        CalibrationReport {
            schema_version: 1,
            source: source.to_string(),
            generated_at: chrono::Utc::now().to_rfc3339(),
            metrics,
            notes,
        }
    }
}

pub fn ingest_row(agg: &mut Aggregator, r: &TestResultRow) {
    if let Some(obj) = r.details.get("metrics").and_then(|m| m.as_object()) {
        for (metric_name, mv) in obj {
            if let Some(score) = mv.get("score").and_then(|s| s.as_f64()) {
                // Aggregate globally for the metric type
                let score = score.clamp(-1.0, 1.0);
                agg.push(
                    MetricKey {
                        metric: metric_name.clone(),
                        test_id: None,
                    },
                    score,
                );

                // Uncomment if per-test granularity is requested via flag
                // agg.push(MetricKey { metric: metric_name.clone(), test_id: Some(r.test_id.clone()) }, score);
            }
        }
    }
}

fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let n = sorted.len() as f64;
    // index-percentile (floor) / nearest-rank (stable)
    let idx = ((q * (n - 1.0)).floor() as usize).min(sorted.len() - 1);
    sorted[idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_percentiles() {
        let data = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0];
        // p10 of 10 items (indices 0..9) -> 0.1 * 9 = 0.9 -> floor -> 0.
        // Index 0 is 0.1.
        assert_eq!(percentile(&data, 0.10), 0.1);

        // p50 -> 0.5 * 9 = 4.5 -> floor -> 4.
        // Index 4 is 0.5.
        assert_eq!(percentile(&data, 0.50), 0.5);

        // p90 -> 0.9 * 9 = 8.1 -> floor -> 8.
        // Index 8 is 0.9.
        assert_eq!(percentile(&data, 0.90), 0.9);
    }
}
