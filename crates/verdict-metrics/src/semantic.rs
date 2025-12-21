use async_trait::async_trait;
use verdict_core::embeddings::util::cosine_similarity;
use verdict_core::metrics_api::{Metric, MetricResult};
use verdict_core::model::{Expected, LlmResponse, TestCase};

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
            .pointer("/verdict/embeddings/response")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("config error: missing response embedding for semantic similarity. Ensure embedder is configured or trace contains embeddings."))?;

        let b = resp
            .meta
            .pointer("/verdict/embeddings/reference")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                anyhow::anyhow!("config error: missing reference embedding for semantic similarity")
            })?;

        // Convert JSON -> f32 with strict checking
        let va: Vec<f32> = a
            .iter()
            .map(|x| {
                x.as_f64().ok_or_else(|| {
                    anyhow::anyhow!("config error: embedding (response) contains non-numeric value")
                })
            })
            .collect::<Result<Vec<f64>, _>>()?
            .into_iter()
            .map(|x| x as f32)
            .collect();

        let vb: Vec<f32> = b
            .iter()
            .map(|x| {
                x.as_f64().ok_or_else(|| {
                    anyhow::anyhow!(
                        "config error: embedding (reference) contains non-numeric value"
                    )
                })
            })
            .collect::<Result<Vec<f64>, _>>()?
            .into_iter()
            .map(|x| x as f32)
            .collect();

        let score = cosine_similarity(&va, &vb)?;
        let passed = score >= *min_score;

        Ok(MetricResult {
            score,
            passed,
            unstable: false,
            details: serde_json::json!({
                "score": score,
                "min_score": min_score,
                "dims": va.len(),
                "model": resp.meta.pointer("/verdict/embeddings/model"),
                "source_response": resp.meta.pointer("/verdict/embeddings/source_response"),
                "source_reference": resp.meta.pointer("/verdict/embeddings/source_reference")
            }),
        })
    }
}
