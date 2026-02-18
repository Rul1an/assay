use crate::cli::args::SimSoakArgs;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SoakReportV1 {
    pub schema_version: String,
    pub report_version: u32,
    pub assay_version: String,
    pub run: RunInfo,
    pub trace: TraceInfo,
    pub target: TargetInfo,
    pub decision_policy: DecisionPolicy,
    pub summary: Summary,
    pub trials: Vec<TrialResult>,
    pub violations: Vec<RuleViolation>,
    pub canaries: Vec<CanarySignal>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RunInfo {
    pub run_id: String,
    pub started_at_utc: String,
    pub duration_seconds: u64,
    pub seed: u64,
    pub iterations: u32,
    pub time_budget_seconds: u64,
    pub evaluation_unit: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TraceInfo {
    pub traceparent: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TargetInfo {
    pub bundle_id: String,
    pub bundle_digest: String,
    pub pack_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DecisionPolicy {
    pub name: String,
    pub thresholds: serde_json::Value,
    pub fail_mode: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Summary {
    pub pass_all: bool,
    pub pass_rate: f64,
    pub ci_95: [f64; 2],
    pub dimensions: Dimensions,
    pub thresholds_used: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Dimensions {
    pub correctness: DimensionStats,
    pub safety: DimensionStats,
    pub security: DimensionStats,
    pub control: DimensionStats,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DimensionStats {
    pub pass_rate: f64,
    pub pass_count: u32,
    pub fail_count: u32,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TrialResult {
    pub index: u32,
    pub seed: u64,
    pub outcome: String,
    pub duration_seconds: f64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RuleViolation {
    pub rule_id: String,
    pub dimension: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CanarySignal {
    pub name: String,
    pub status: String,
    pub trigger_count: u32,
}

pub(crate) fn build_report(assay_version: &str, args: &SimSoakArgs) -> SoakReportV1 {
    let seed = args.seed.unwrap_or(0);
    let iterations = args.iterations;

    let mut trials = Vec::with_capacity(iterations as usize);
    for i in 1..=iterations {
        let outcome = if seed > 0 && i % 10 == 0 {
            "fail"
        } else {
            "pass"
        };
        trials.push(TrialResult {
            index: i,
            seed,
            outcome: outcome.to_string(),
            duration_seconds: 1.0,
        });
    }

    let failed = trials.iter().filter(|t| t.outcome == "fail").count() as u32;
    let passed = iterations.saturating_sub(failed);
    let pass_rate = if iterations == 0 {
        0.0
    } else {
        passed as f64 / iterations as f64
    };

    let violations = if failed > 0 {
        vec![RuleViolation {
            rule_id: "soak.synthetic.failure".to_string(),
            dimension: "correctness".to_string(),
            count: failed,
        }]
    } else {
        Vec::new()
    };

    SoakReportV1 {
        schema_version: "soak-report-v1".to_string(),
        report_version: 1,
        assay_version: assay_version.to_string(),
        run: RunInfo {
            run_id: format!("soak-{}", seed),
            started_at_utc: "1970-01-01T00:00:00Z".to_string(),
            duration_seconds: iterations as u64,
            seed,
            iterations,
            time_budget_seconds: args.time_budget,
            evaluation_unit: "scenario".to_string(),
        },
        trace: TraceInfo {
            traceparent: "00-00000000000000000000000000000000-0000000000000000-00".to_string(),
        },
        target: TargetInfo {
            bundle_id: args.target.clone(),
            bundle_digest: "sha256:unknown".to_string(),
            pack_ids: vec!["default-pack".to_string()],
        },
        decision_policy: DecisionPolicy {
            name: "adr025-i1".to_string(),
            thresholds: serde_json::json!({ "min_pass_rate": 0.95 }),
            fail_mode: "enforce".to_string(),
        },
        summary: Summary {
            pass_all: failed == 0,
            pass_rate,
            ci_95: [pass_rate, pass_rate],
            dimensions: Dimensions {
                correctness: DimensionStats {
                    pass_rate,
                    pass_count: passed,
                    fail_count: failed,
                },
                safety: DimensionStats {
                    pass_rate,
                    pass_count: passed,
                    fail_count: failed,
                },
                security: DimensionStats {
                    pass_rate,
                    pass_count: passed,
                    fail_count: failed,
                },
                control: DimensionStats {
                    pass_rate,
                    pass_count: passed,
                    fail_count: failed,
                },
            },
            thresholds_used: serde_json::json!({ "min_pass_rate": 0.95 }),
        },
        trials,
        violations,
        canaries: vec![CanarySignal {
            name: "synthetic-baseline".to_string(),
            status: "pass".to_string(),
            trigger_count: 0,
        }],
    }
}

pub(crate) fn measurement_exceeded(report: &SoakReportV1) -> bool {
    report.run.duration_seconds > report.run.time_budget_seconds
}
