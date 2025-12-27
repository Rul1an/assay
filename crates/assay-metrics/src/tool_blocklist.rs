use assay_core::metrics_api::{Metric, MetricResult};
use assay_core::model::{Expected, LlmResponse, TestCase, ToolCallRecord};
use async_trait::async_trait;

pub struct ToolBlocklistMetric;

#[async_trait]
impl Metric for ToolBlocklistMetric {
    fn name(&self) -> &'static str {
        "tool_blocklist"
    }

    async fn evaluate(
        &self,
        _tc: &TestCase,
        expected: &Expected,
        resp: &LlmResponse,
    ) -> anyhow::Result<MetricResult> {
        let blocked = match expected {
            Expected::ToolBlocklist { blocked } => blocked,
            _ => return Ok(MetricResult::pass(1.0)), // N/A
        };

        let tool_calls: Vec<ToolCallRecord> = if let Some(val) = resp.meta.get("tool_calls") {
            serde_json::from_value(val.clone()).unwrap_or_default()
        } else {
            Vec::new()
        };

        for call in tool_calls {
            if blocked.contains(&call.tool_name) {
                return Ok(MetricResult::fail(
                    0.0,
                    &format!("Blocked tool called: {}", call.tool_name),
                ));
            }
        }

        Ok(MetricResult::pass(1.0))
    }
}
