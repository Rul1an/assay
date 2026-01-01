use assay_core::metrics_api::{Metric, MetricResult};
use assay_core::model::{Expected, LlmResponse, TestCase, ToolCallRecord};
use async_trait::async_trait;

pub struct SequenceValidMetric;

#[async_trait]
impl Metric for SequenceValidMetric {
    fn name(&self) -> &'static str {
        "sequence_valid"
    }

    async fn evaluate(
        &self,
        _tc: &TestCase,
        expected: &Expected,
        resp: &LlmResponse,
    ) -> anyhow::Result<MetricResult> {
        let (policy_path, inline_sequence, inline_rules) = match expected {
            Expected::SequenceValid {
                policy,
                sequence,
                rules,
            } => (policy, sequence, rules),
            _ => return Ok(MetricResult::pass(1.0)),
        };

        // 1. Resolve Rules & Sequence from Policy File (if any)
        let (file_sequence, file_rules) = if let Some(path) = policy_path {
            static WARN_ONCE: std::sync::Once = std::sync::Once::new();
            WARN_ONCE.call_once(|| {
                if std::env::var("MCP_CONFIG_LEGACY").is_err() {
                    eprintln!("WARN: Deprecated policy file '{}' detected. Please migrate to inline usage.", path);
                    eprintln!("      To suppress this, set MCP_CONFIG_LEGACY=1 or run 'assay migrate'.");
                }
            });

            let content = std::fs::read_to_string(path).map_err(|e| {
                anyhow::anyhow!(
                    "config error: failed to read sequence_valid policy '{}': {}",
                    path,
                    e
                )
            })?;

            // Try parsing as list of strings (legacy sequence)
            if let Ok(seq) = serde_yaml::from_str::<Vec<String>>(&content) {
                (Some(seq), None)
            } else if let Ok(pol) = serde_yaml::from_str::<assay_core::model::Policy>(&content) {
                (None, Some(pol.sequences))
            } else {
                // Try parsing as list of rules
                let rules = serde_yaml::from_str::<Vec<assay_core::model::SequenceRule>>(&content)
                    .map_err(|e| anyhow::anyhow!("config error: invalid sequence_valid policy '{}'. Expected list of strings or list of rules. Error: {}", path, e))?;
                (None, Some(rules))
            }
        } else {
            (None, None)
        };

        let effective_sequence = inline_sequence.as_ref().or(file_sequence.as_ref());
        let effective_rules = inline_rules.as_ref().or(file_rules.as_ref());

        if effective_sequence.is_none() && effective_rules.is_none() {
            return Ok(MetricResult::pass(1.0));
        }

        // Parse Tool Calls
        let tool_calls: Vec<ToolCallRecord> = if let Some(val) = resp.meta.get("tool_calls") {
            serde_json::from_value(val.clone()).unwrap_or_default()
        } else {
            Vec::new()
        };

        // Sort by index
        let mut actual_sequence = tool_calls.clone();
        actual_sequence.sort_by_key(|k| k.index);
        let actual_names: Vec<String> = actual_sequence
            .iter()
            .map(|c| c.tool_name.clone())
            .collect();

        // 2. Validate Rules (DSL)
        if let Some(rules) = effective_rules {
            for rule in rules {
                match rule {
                    assay_core::model::SequenceRule::Require { tool } => {
                        if !actual_names.contains(tool) {
                            return Ok(MetricResult::fail(
                                0.0,
                                &format!("sequence_valid rule failed: required tool '{}' not found in trace", tool)
                            ));
                        }
                    }
                    assay_core::model::SequenceRule::Before { first, then } => {
                        let first_idx = actual_names.iter().position(|n| n == first);
                        let then_idx = actual_names.iter().position(|n| n == then);

                        // "Before" implies: IF 'then' is present, 'first' MUST be present AND occur before it.
                        // (Strict dependency: you can't have B without A)
                        if let Some(t_idx) = then_idx {
                            if let Some(f_idx) = first_idx {
                                if f_idx > t_idx {
                                    return Ok(MetricResult::fail(
                                        0.0,
                                        &format!("sequence_valid rule failed: tool '{}' appeared at index {} but was required before tool '{}' (index {})",
                                            first, f_idx, then, t_idx)
                                    ));
                                }
                            } else {
                                return Ok(MetricResult::fail(
                                    0.0,
                                    &format!("sequence_valid rule failed: tool '{}' was found (index {}) but required preceding tool '{}' was missing",
                                        then, t_idx, first)
                                ));
                            }
                        }
                    }
                    assay_core::model::SequenceRule::Blocklist { pattern } => {
                        // Simple substring blocklist for now, or full regex if needed
                        for name in &actual_names {
                            if name.contains(pattern) {
                                return Ok(MetricResult::fail(
                                    0.0,
                                    &format!("sequence_valid rule failed: tool '{}' matches blocklist pattern '{}'", name, pattern)
                                ));
                            }
                        }
                    }
                    _ => {
                        // TODO: Implement v1.1 operators (Eventually, MaxCalls, etc)
                        // Note: Consider delegating to assay-core::explain::TraceExplainer once stabilized.
                    }
                }
            }
        }

        // 3. Validate Exact Sequence (Legacy / Strict)
        if let Some(expected_sequence) = effective_sequence {
            if actual_names == *expected_sequence {
                return Ok(MetricResult::pass(1.0));
            } else {
                let mut diff_context = String::new();
                let limit = std::cmp::min(actual_names.len(), expected_sequence.len());
                for i in 0..limit {
                    if actual_names[i] != expected_sequence[i] {
                        diff_context = format!(
                            "Mismatch at index [{}]: Expected '{}', Found '{}'",
                            i, expected_sequence[i], actual_names[i]
                        );
                        break;
                    }
                }
                if diff_context.is_empty() {
                    if actual_names.len() > expected_sequence.len() {
                        diff_context = format!(
                            "Unexpected extra tool at index [{}]: '{}'",
                            expected_sequence.len(),
                            actual_names[expected_sequence.len()]
                        );
                    } else {
                        diff_context = format!(
                            "Missing expected tool at index [{}]: '{}'",
                            actual_names.len(),
                            expected_sequence[actual_names.len()]
                        );
                    }
                }
                return Ok(MetricResult::fail(
                    0.0,
                    &format!(
                        "sequence_valid mismatch. {}, (Expected {}: {:?}, Actual {}: {:?})",
                        diff_context,
                        expected_sequence.len(),
                        expected_sequence,
                        actual_names.len(),
                        actual_names
                    ),
                ));
            }
        }

        Ok(MetricResult::pass(1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assay_core::model::{SequenceRule, TestInput};

    fn make_test_case(actual_tools: Vec<&str>) -> (TestCase, LlmResponse) {
        let tc = TestCase {
            id: "test".to_string(),
            input: TestInput {
                prompt: "prompt".to_string(),
                context: None,
            },
            expected: Expected::MustContain {
                must_contain: vec![],
            },
            assertions: None,
            tags: vec![],
            metadata: None,
            on_error: None,
        };
        let mut meta = serde_json::Map::new();
        let tool_calls: Vec<ToolCallRecord> = actual_tools
            .into_iter()
            .enumerate()
            .map(|(i, name)| ToolCallRecord {
                id: format!("call-{}", i),
                tool_name: name.to_string(),
                args: serde_json::json!({}),
                result: None,
                error: None,
                index: i,
                ts_ms: 100 * i as u64,
            })
            .collect();
        meta.insert(
            "tool_calls".to_string(),
            serde_json::to_value(tool_calls).unwrap(),
        );

        let resp = LlmResponse {
            meta: serde_json::Value::Object(meta),
            ..Default::default()
        };
        (tc, resp)
    }

    #[tokio::test]
    async fn test_passes_when_in_order() {
        let metric = SequenceValidMetric;
        let (tc, resp) = make_test_case(vec!["A", "B", "C"]);
        let expected = Expected::SequenceValid {
            policy: None,
            sequence: None,
            rules: Some(vec![SequenceRule::Before {
                first: "B".to_string(),
                then: "C".to_string(),
            }]),
        };

        let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
        assert_eq!(result.score, 1.0, "Should pass when B is before C");
    }

    #[tokio::test]
    async fn test_fails_when_missing_required() {
        let metric = SequenceValidMetric;
        let (tc, resp) = make_test_case(vec!["A", "C"]); // Missing B
        let expected = Expected::SequenceValid {
            policy: None,
            sequence: None,
            rules: Some(vec![SequenceRule::Require {
                tool: "B".to_string(),
            }]),
        };

        let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
        assert_eq!(result.score, 0.0, "Should fail when B is missing");

        let details = result.details.as_object().unwrap();
        let msg = details.get("message").and_then(|v| v.as_str()).unwrap();
        assert!(msg.contains("required tool 'B' not found"), "Msg: {}", msg);
    }

    #[tokio::test]
    async fn test_fails_when_out_of_order() {
        let metric = SequenceValidMetric;
        let (tc, resp) = make_test_case(vec!["A", "C", "B"]); // C before B
        let expected = Expected::SequenceValid {
            policy: None,
            sequence: None,
            rules: Some(vec![SequenceRule::Before {
                first: "B".to_string(),
                then: "C".to_string(),
            }]),
        };

        let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
        assert_eq!(result.score, 0.0, "Should fail when B is after C");

        let details = result.details.as_object().unwrap();
        let msg = details.get("message").and_then(|v| v.as_str()).unwrap();
        assert!(msg.contains("was required before tool 'C'"), "Msg: {}", msg);
    }

    #[tokio::test]
    async fn test_blocklist_rule() {
        let metric = SequenceValidMetric;
        let (tc, resp) = make_test_case(vec!["A", "rm -rf"]);
        let expected = Expected::SequenceValid {
            policy: None,
            sequence: None,
            rules: Some(vec![SequenceRule::Blocklist {
                pattern: "rm".to_string(),
            }]),
        };
        let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
        assert_eq!(result.score, 0.0);
        let msg = result.details["message"].as_str().unwrap();
        assert!(msg.contains("matches blocklist pattern 'rm'"));
    }
}
