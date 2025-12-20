use crate::model::{Expected, LlmResponse, TestCase};
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct MetricResult {
    pub score: f64,
    pub passed: bool,
    pub unstable: bool,
    pub details: serde_json::Value,
}

impl MetricResult {
    pub fn pass(score: f64) -> Self {
        Self {
            score,
            passed: true,
            unstable: false,
            details: serde_json::json!({}),
        }
    }
    pub fn fail(score: f64, msg: &str) -> Self {
        Self {
            score,
            passed: false,
            unstable: false,
            details: serde_json::json!({"message": msg}),
        }
    }
    pub fn unstable(score: f64, msg: &str) -> Self {
        Self {
            score,
            passed: false,
            unstable: true,
            details: serde_json::json!({"message": msg}),
        }
    }
}

#[async_trait]
pub trait Metric: Send + Sync {
    fn name(&self) -> &'static str;
    async fn evaluate(
        &self,
        tc: &TestCase,
        expected: &Expected,
        resp: &LlmResponse,
    ) -> anyhow::Result<MetricResult>;
}
