use assay_core::metrics_api::{Metric, MetricResult};
use assay_core::model::{Expected, LlmResponse, TestCase};
use async_trait::async_trait;

const EPSILON: f64 = 1e-9;

pub struct FaithfulnessMetric;

#[async_trait]
impl Metric for FaithfulnessMetric {
    fn name(&self) -> &'static str {
        "faithfulness"
    }

    async fn evaluate(
        &self,
        _tc: &TestCase,
        expected: &Expected,
        output: &LlmResponse,
    ) -> anyhow::Result<MetricResult> {
        let (min_score, rubric_version) = match expected {
            Expected::Faithfulness {
                min_score,
                rubric_version,
                ..
            } => (*min_score, rubric_version),
            _ => return Ok(MetricResult::pass(1.0)),
        };

        evaluate_judge_result("faithfulness", min_score, rubric_version.as_deref(), output)
    }
}

pub struct RelevanceMetric;

#[async_trait]
impl Metric for RelevanceMetric {
    fn name(&self) -> &'static str {
        "relevance"
    }

    async fn evaluate(
        &self,
        _tc: &TestCase,
        expected: &Expected,
        output: &LlmResponse,
    ) -> anyhow::Result<MetricResult> {
        let (min_score, rubric_version) = match expected {
            Expected::Relevance {
                min_score,
                rubric_version,
                ..
            } => (*min_score, rubric_version),
            _ => return Ok(MetricResult::pass(1.0)),
        };

        evaluate_judge_result("relevance", min_score, rubric_version.as_deref(), output)
    }
}

fn evaluate_judge_result(
    rubric_id: &str,
    min_score: f64,
    _rubric_version: Option<&str>,
    output: &LlmResponse,
) -> anyhow::Result<MetricResult> {
    let judge_data = output.meta.pointer(&format!("/assay/judge/{}", rubric_id));

    let Some(data) = judge_data else {
        // Judge result missing
        return Ok(MetricResult {
            passed: false,
            score: 0.0,
            details: serde_json::json!({
                "message": "config error: judge result missing (judge disabled or not in trace)"
            }),
            unstable: false,
        });
    };

    let passed_bool = data
        .get("passed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let score = data.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
    // Rationale is in data, but we don't put it in the message string to avoid leaking it in unredacted logs/junit
    let _rationale = data.get("rationale").and_then(|v| v.as_str()).unwrap_or("");

    // Check threshold with epsilon
    let threshold_pass = score + EPSILON >= min_score;
    // The "passed" field in judge result is majority vote.
    // Score is agreement ratio (0.0 to 1.0).

    // We trust "passed" boolean from judge service (majority vote),
    // BUT we also respect min_score if user set it high (e.g. requires 1.0 agreement).
    let passed = passed_bool && threshold_pass;

    let message = if passed {
        "passed".into()
    } else {
        format!("failed judge check (score={:.2})", score)
    };

    // Clone data and inject message
    let mut details = data.clone();
    if let Some(obj) = details.as_object_mut() {
        obj.insert("message".to_string(), serde_json::Value::String(message));
    }

    Ok(MetricResult {
        passed,
        score,
        details,
        unstable: score > 0.0 && score < 1.0, // Agreement < 1.0 implies instability/disagreement
    })
}
