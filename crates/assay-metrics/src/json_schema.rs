use assay_core::metrics_api::{Metric, MetricResult};
use assay_core::model::{Expected, LlmResponse, TestCase};
use async_trait::async_trait;
use jsonschema::JSONSchema;
use std::sync::Arc;

pub struct JsonSchemaMetric;

#[async_trait]
impl Metric for JsonSchemaMetric {
    fn name(&self) -> &'static str {
        "json_schema"
    }

    async fn evaluate(
        &self,
        tc: &TestCase,
        expected: &Expected,
        resp: &LlmResponse,
    ) -> anyhow::Result<MetricResult> {
        let Expected::JsonSchema {
            json_schema,
            schema_file,
        } = expected
        else {
            return Ok(MetricResult::pass(1.0));
        };

        let schema_str = if let Some(path) = schema_file {
            std::fs::read_to_string(path).map_err(|e| {
                let origin = tc
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("assay"))
                    .and_then(|v| v.get("schema_file_original"))
                    .and_then(|v| v.as_str());

                if let Some(o) = origin {
                    anyhow::anyhow!(
                        "config error: failed to read schema_file '{}' (resolved from '{}'): {}",
                        path,
                        o,
                        e
                    )
                } else {
                    anyhow::anyhow!("config error: failed to read schema_file '{}': {}", path, e)
                }
            })?
        } else {
            if json_schema.trim().is_empty() {
                return Err(anyhow::anyhow!(
                    "config error: missing json_schema or schema_file"
                ));
            }
            json_schema.clone()
        };

        let schema_json: serde_json::Value = serde_json::from_str(&schema_str)
            .map_err(|e| anyhow::anyhow!("config error: invalid JSON schema: {}", e))?;

        let compiled = JSONSchema::options()
            .compile(&schema_json)
            .map_err(|e| anyhow::anyhow!("config error: schema compile failed: {}", e))?;

        let instance: serde_json::Value = match serde_json::from_str(&resp.text) {
            Ok(v) => v,
            Err(_) => {
                return Ok(MetricResult::fail(
                    0.0,
                    "json_schema failed: response is not valid JSON",
                ));
            }
        };

        let result = compiled.validate(&instance);
        if let Err(errors) = result {
            let error_list: Vec<String> = errors.map(|e| e.to_string()).collect();
            Ok(MetricResult {
                score: 0.0,
                passed: false,
                unstable: false,
                details: serde_json::json!({
                    "message": format!("json_schema failed: {} validation errors", error_list.len()),
                    "errors": error_list
                }),
            })
        } else {
            Ok(MetricResult::pass(1.0))
        }
    }
}

pub fn metric() -> Arc<dyn Metric> {
    Arc::new(JsonSchemaMetric)
}
