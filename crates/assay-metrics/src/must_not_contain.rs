use assay_core::metrics_api::{Metric, MetricResult};
use assay_core::model::{Expected, LlmResponse, TestCase};
use async_trait::async_trait;

pub struct MustNotContainMetric;

#[async_trait]
impl Metric for MustNotContainMetric {
    fn name(&self) -> &'static str {
        "must_not_contain"
    }

    async fn evaluate(
        &self,
        _tc: &TestCase,
        expected: &Expected,
        resp: &LlmResponse,
    ) -> anyhow::Result<MetricResult> {
        let Expected::MustNotContain { must_not_contain } = expected else {
            return Ok(MetricResult::pass(1.0));
        };
        for s in must_not_contain {
            if resp.text.contains(s) {
                return Ok(MetricResult::fail(
                    0.0,
                    &format!("forbidden substring present: {}", s),
                ));
            }
        }
        Ok(MetricResult::pass(1.0))
    }
}
