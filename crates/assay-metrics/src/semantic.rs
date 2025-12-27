use assay_core::embeddings::util::cosine_similarity_f64;
use assay_core::metrics_api::{Metric, MetricResult};
use assay_core::model::{Expected, LlmResponse, TestCase};
use async_trait::async_trait;

const EPSILON: f64 = 1e-6;

pub struct SemanticSimilarityMetric;

#[async_trait]
impl Metric for SemanticSimilarityMetric {
    fn name(&self) -> &'static str {
        "semantic_similarity_to"
    }

    async fn evaluate(
        &self,
        _tc: &TestCase,
        expected: &Expected,
        resp: &LlmResponse,
    ) -> anyhow::Result<MetricResult> {
        let Expected::SemanticSimilarityTo { min_score, .. } = expected else {
            return Ok(MetricResult::pass(1.0));
        };

        let a = resp
            .meta
            .pointer("/assay/embeddings/response")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("config error: missing response embedding for semantic similarity. Ensure embedder is configured or trace contains embeddings."))?;

        let b = resp
            .meta
            .pointer("/assay/embeddings/reference")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                anyhow::anyhow!("config error: missing reference embedding for semantic similarity")
            })?;

        // Convert JSON -> f64 with strict checking
        let va: Vec<f64> = a
            .iter()
            .map(|x| {
                x.as_f64().ok_or_else(|| {
                    anyhow::anyhow!("config error: embedding (response) contains non-numeric value")
                })
            })
            .collect::<Result<Vec<f64>, _>>()?;

        let vb: Vec<f64> = b
            .iter()
            .map(|x| {
                x.as_f64().ok_or_else(|| {
                    anyhow::anyhow!(
                        "config error: embedding (reference) contains non-numeric value"
                    )
                })
            })
            .collect::<Result<Vec<f64>, _>>()?;

        let score = cosine_similarity_f64(&va, &vb)?;

        // Guard against tiny floating point rounding differences near the threshold.
        // Scores within EPSILON of the threshold are treated as passing.
        let passed = score + EPSILON >= *min_score;

        Ok(MetricResult {
            score,
            passed,
            unstable: false,
            details: serde_json::json!({
                "score": score,
                "min_score": min_score,
                "epsilon": EPSILON,
                "dims": va.len(),
                "model": resp.meta.pointer("/assay/embeddings/model"),
                "source_response": resp.meta.pointer("/assay/embeddings/source_response"),
                "source_reference": resp.meta.pointer("/assay/embeddings/source_reference")
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assay_core::model::{LlmResponse, TestInput};

    fn make_test_case(
        min_score: f64,
        response_vec: &[f64],
        ref_vec: &[f64],
    ) -> (TestCase, Expected, LlmResponse) {
        let tc = TestCase {
            id: "test".into(),
            input: TestInput {
                prompt: "test".into(),
                context: None,
            },
            expected: Expected::SemanticSimilarityTo {
                semantic_similarity_to: "hello world".to_string(),
                min_score,
                thresholding: None,
            },
            tags: vec![],
            metadata: None,
            assertions: None,
        };
        let expected = tc.expected.clone();
        let resp = LlmResponse {
            text: "resp".into(),
            model: "model".into(),
            provider: "test".into(),
            cached: false,
            meta: serde_json::json!({
                "assay": {
                    "embeddings": {
                        "response": response_vec,
                        "reference": ref_vec,
                        "model": "test",
                        "source_response": "test",
                        "source_reference": "test"
                    }
                }
            }),
        };
        (tc, expected, resp)
    }

    #[tokio::test]
    async fn test_boundary_pass() {
        // Score = 1.0. Threshold = 1.0 + epsilon/2 (impossible physically but tests logic)
        // Let's use cosine of same vectors = 1.0.
        // Set threshold to 1.0 + 5e-7 (which is > 1.0).
        // Score (1.0) + 1e-6 (10e-7) >= 1.0 + 5e-7. Should pass.
        // Actually, let's use user example: score = threshold - 5e-7 -> pass.
        // If threshold is 0.8. We want score 0.7999995.
        // Creating exact vectors for that cosine is hard.
        // Instead I'll verify the logic by trusting cosine_similarity_f64 results
        // will be exactly comparable to injected floats if I use dummy logic?
        // No, I can't inject score directly.
        // I'll rely on the logic:
        // Case: Identical vectors (score 1.0).
        // Threshold: 1.0 (should pass).

        let metric = SemanticSimilarityMetric;
        let v = vec![1.0, 0.0];
        let (tc, expected, resp) = make_test_case(1.0, &v, &v);
        let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
        assert!(result.passed);
        assert!((result.score - 1.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn test_boundary_epsilon_guard() {
        // Case: Score is exactly 1.0 (identical vectors).
        // Threshold is set slightly ABOVE 1.0 (e.g. 1.0 + 0.5 * EPSILON).
        // Without epsilon, 1.0 < 1.0000005 -> FAIL.
        // With epsilon, 1.0 + 1e-6 >= 1.0000005 -> PASS.

        let metric = SemanticSimilarityMetric;
        let v = vec![1.0, 0.0];
        // Threshold = 1.0 + 5e-7
        let threshold = 1.0 + (0.5 * EPSILON);

        let (tc, expected, resp) = make_test_case(threshold, &v, &v);
        let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();

        assert!(
            result.passed,
            "Score 1.0 should pass threshold 1.0 + 0.5*EPSILON due to guard"
        );

        // Sanity check: verify it fails if threshold is too high
        // Threshold = 1.0 + 2.0 * EPSILON
        let (tc_fail, expected_fail, resp_fail) = make_test_case(1.0 + (2.0 * EPSILON), &v, &v);
        let result_fail = metric
            .evaluate(&tc_fail, &expected_fail, &resp_fail)
            .await
            .unwrap();
        assert!(
            !result_fail.passed,
            "Score 1.0 should fail threshold 1.0 + 2*EPSILON"
        );
    }
}
