use assay_core::metrics_api::{Metric, MetricResult};
use assay_core::model::{Expected, LlmResponse, TestCase};
use async_trait::async_trait;
use regex::RegexBuilder;
use std::sync::Arc;

pub struct RegexMatchMetric;

#[async_trait]
impl Metric for RegexMatchMetric {
    fn name(&self) -> &'static str {
        "regex_match"
    }

    async fn evaluate(
        &self,
        _tc: &TestCase,
        expected: &Expected,
        resp: &LlmResponse,
    ) -> anyhow::Result<MetricResult> {
        let (pattern, flags, negate) = match expected {
            Expected::RegexMatch { pattern, flags } => (pattern.as_str(), flags.as_slice(), false),
            Expected::RegexNotMatch { pattern, flags } => {
                (pattern.as_str(), flags.as_slice(), true)
            }
            _ => return Ok(MetricResult::pass(1.0)),
        };

        let mut b = RegexBuilder::new(pattern);
        apply_flags(&mut b, flags);

        let re = b.build().map_err(|e| {
            anyhow::anyhow!("config error: invalid regex pattern '{}': {}", pattern, e)
        })?;

        let is_match = re.is_match(&resp.text);
        if negate {
            if is_match {
                return Ok(MetricResult::fail(
                    0.0,
                    &format!("regex_not_match violated: pattern '{}' matched", pattern),
                ));
            }
            Ok(MetricResult::pass(1.0))
        } else {
            if !is_match {
                return Ok(MetricResult::fail(
                    0.0,
                    &format!("regex_match failed: pattern '{}' did not match", pattern),
                ));
            }
            Ok(MetricResult::pass(1.0))
        }
    }
}

fn apply_flags(b: &mut RegexBuilder, flags: &[String]) {
    for f in flags {
        match f.as_str() {
            "i" => {
                b.case_insensitive(true);
            }
            "m" => {
                b.multi_line(true);
            }
            "s" => {
                b.dot_matches_new_line(true);
            }
            _ => {
                // Ignore unknown flags for now to be safe
            }
        }
    }
}

pub fn metric() -> Arc<dyn Metric> {
    Arc::new(RegexMatchMetric)
}
