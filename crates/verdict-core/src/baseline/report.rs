use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct HygieneReport {
    pub schema_version: u32,
    pub suite: String,
    pub source: String,
    pub score_source: String, // "final_attempt" or "all_attempts"
    pub generated_at: String,
    pub window: ReportWindow,
    pub tests: Vec<TestHygiene>,
    pub notes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReportWindow {
    pub last_runs: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TestHygiene {
    pub test_id: String,
    pub n: u32,
    pub rates: TestOutcomeRates,
    pub scores: HashMap<String, MetricStats>,
    pub top_reasons: Vec<TopReason>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggested_actions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TestOutcomeRates {
    pub pass: f64,
    pub fail: f64,
    pub warn: f64,
    pub flaky: f64,
    pub unstable: f64,
    pub skipped: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MetricStats {
    pub p10: f64,
    pub p50: f64,
    pub p90: f64,
    pub std: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TopReason {
    pub kind: String, // "skip_reason" or "error" or "failure"
    pub value: String,
    pub count: u32,
}

use crate::model::{TestResultRow, TestStatus};
use crate::storage::Store;

pub fn report_from_db(store: &Store, suite: &str, last_runs: u32) -> anyhow::Result<HygieneReport> {
    let results = store.fetch_results_for_last_n_runs(suite, last_runs)?;

    // Group by test_id
    let mut test_groups: HashMap<String, Vec<&TestResultRow>> = HashMap::new();
    for r in &results {
        test_groups.entry(r.test_id.clone()).or_default().push(r);
    }

    let mut tests = Vec::new();
    let mut notes = Vec::new();

    for (test_id, rows) in test_groups {
        let n = rows.len() as u32;
        let mut counts = HashMap::new();
        let mut reasons = HashMap::new(); // Key: (kind, value) -> Count
        let mut scores: HashMap<String, Vec<f64>> = HashMap::new();

        for r in &rows {
            *counts.entry(r.status.clone()).or_insert(0) += 1;

            // Collect reasons
            if let Some(reason) = &r.skip_reason {
                *reasons
                    .entry(("skip_reason".to_string(), reason.clone()))
                    .or_insert(0) += 1;
            } else if r.status == TestStatus::Fail || r.status == TestStatus::Error {
                // Primary failure reason
                let msg = if r.message.is_empty() {
                    "Undeclared failure".to_string()
                } else {
                    r.message.clone()
                };
                *reasons.entry(("failure".to_string(), msg)).or_insert(0) += 1;
            }

            // Extract granular metric reasons (regardless of status, often informative)
            // Look at final result details first
            if let Some(obj) = r.details.get("metrics").and_then(|m| m.as_object()) {
                for (metric_name, mv) in obj {
                    // If metric has a 'reason' string
                    if let Some(reason) = mv.get("reason").and_then(|s| s.as_str()) {
                        let key = format!("{}: {}", metric_name, reason);
                        *reasons
                            .entry(("metric_reason".to_string(), key))
                            .or_insert(0) += 1;
                    }
                }
            }

            // Collect metrics scores: Use *all* attempts for robust statistics if available
            if let Some(attempts) = &r.attempts {
                if !attempts.is_empty() {
                    for attempt in attempts {
                        if let Some(obj) =
                            attempt.details.get("metrics").and_then(|m| m.as_object())
                        {
                            for (metric_name, mv) in obj {
                                if let Some(score) = mv.get("score").and_then(|s| s.as_f64()) {
                                    scores.entry(metric_name.clone()).or_default().push(score);
                                }
                            }
                        }
                    }
                } else {
                    // Fallback to result details if attempts logical but empty (shouldn't happen with updated store query)
                    // or if we decide to stick to final. Actually store.rs ensures attempts is populated.
                    // But for safety:
                    if let Some(obj) = r.details.get("metrics").and_then(|m| m.as_object()) {
                        for (metric_name, mv) in obj {
                            if let Some(score) = mv.get("score").and_then(|s| s.as_f64()) {
                                scores.entry(metric_name.clone()).or_default().push(score);
                            }
                        }
                    }
                }
            } else {
                // Fallback for old records without attempts
                if let Some(obj) = r.details.get("metrics").and_then(|m| m.as_object()) {
                    for (metric_name, mv) in obj {
                        if let Some(score) = mv.get("score").and_then(|s| s.as_f64()) {
                            scores.entry(metric_name.clone()).or_default().push(score);
                        }
                    }
                }
            }
        }

        // Note: Skips are usually status=Pass/Fail but with skip_reason? Or is Skip a status?
        // Verdict core TestStatus enum: Pass, Fail, Error, Warn, Flaky.
        // Skips are recorded but status might be the outcome of the skip? Usually Skip -> Pass in strict logic?
        // Let's rely on skip_reason presence for "skipped" rate if status doesn't capture it.
        // Actually, if skip_reason is present, status is what? usually Pass.

        // Let's refine rates logic
        let skipped_count = rows.iter().filter(|r| r.skip_reason.is_some()).count();
        let rates = TestOutcomeRates {
            pass: (*counts.get(&TestStatus::Pass).unwrap_or(&0) as f64) / n as f64,
            fail: (*counts.get(&TestStatus::Fail).unwrap_or(&0) as f64) / n as f64,
            warn: (*counts.get(&TestStatus::Warn).unwrap_or(&0) as f64) / n as f64,
            flaky: (*counts.get(&TestStatus::Flaky).unwrap_or(&0) as f64) / n as f64,
            unstable: 0.0, // Placeholder
            skipped: skipped_count as f64 / n as f64,
        };

        // Aggregated Scores
        let mut score_stats = HashMap::new();
        for (metric, mut vals) in scores {
            vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let sn = vals.len() as f64;
            if sn == 0.0 {
                continue;
            }

            let sum: f64 = vals.iter().sum();
            let mean = sum / sn;
            let variance = vals.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / sn;
            let std = variance.sqrt();

            // Helper for percentile
            let p = |q: f64| {
                let idx = ((q * (sn - 1.0)).floor() as usize).min(vals.len() - 1);
                vals[idx]
            };

            score_stats.insert(
                metric,
                MetricStats {
                    p10: p(0.10),
                    p50: p(0.50),
                    p90: p(0.90),
                    std,
                },
            );
        }

        // Top Reasons
        let mut top_reasons: Vec<TopReason> = reasons
            .into_iter()
            .map(|((kind, value), count)| TopReason { kind, value, count })
            .collect();
        top_reasons.sort_by(|a, b| b.count.cmp(&a.count));
        top_reasons.truncate(5);

        // Suggested Actions
        let mut actions = Vec::new();
        if rates.skipped > 0.4 {
            actions.push(
                "High skip rate: Check for fingerprint drift or over-aggressive caching"
                    .to_string(),
            );
        }
        if rates.flaky > 0.1 {
            actions.push(
                "Flaky: Consider increasing retries or stabilizing the environment".to_string(),
            );
        }
        if rates.fail > 0.2 {
            actions.push("High failure rate: Investigate top reasons".to_string());
        }
        // Check for low P10 in key metrics
        for (m, stats) in &score_stats {
            if stats.p10 < 0.6 {
                // Heuristic threshold
                actions.push(format!(
                    "Low {} scores (P10 < 0.6): Consider tuning min_score or improving prompts",
                    m
                ));
            }
        }

        tests.push(TestHygiene {
            test_id,
            n,
            rates,
            scores: score_stats,
            top_reasons,
            suggested_actions: actions,
        });
    }

    // Sort tests by fail rate descending (problematic first)
    tests.sort_by(|a, b| {
        b.rates
            .fail
            .partial_cmp(&a.rates.fail)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Global notes
    if tests.iter().any(|t| t.rates.skipped > 0.5) {
        notes.push("High skip rate (>50%) detected in some tests. Check for over-aggressive fingerprinting.".to_string());
    }

    Ok(HygieneReport {
        schema_version: 1,
        suite: suite.to_string(),
        source: "eval.db".to_string(),
        score_source: "all_attempts".to_string(),
        generated_at: chrono::Utc::now().to_rfc3339(),
        window: ReportWindow { last_runs },
        tests,
        notes,
    })
}
