use async_trait::async_trait;
use verdict_core::metrics_api::{Metric, MetricResult};
use verdict_core::model::{Expected, LlmResponse, TestCase};

pub struct MustContainMetric;

#[async_trait]
impl Metric for MustContainMetric {
    fn name(&self) -> &'static str {
        "must_contain"
    }

    async fn evaluate(
        &self,
        _tc: &TestCase,
        expected: &Expected,
        resp: &LlmResponse,
    ) -> anyhow::Result<MetricResult> {
        let Expected::MustContain { must_contain } = expected else {
            return Ok(MetricResult::pass(1.0));
        };
        for s in must_contain {
            if !resp.text.contains(s) {
                return Ok(MetricResult::fail(
                    0.0,
                    &format!("missing substring: {}", s),
                ));
            }
        }
        Ok(MetricResult::pass(1.0))
    }
}
