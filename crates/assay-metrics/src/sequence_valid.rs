use assay_core::metrics_api::{Metric, MetricResult};
use assay_core::model::{Expected, LlmResponse, TestCase, ToolCallRecord};
use async_trait::async_trait;

use crate::policy_warning::should_emit_deprecated_policy_warning;
use crate::tool_calls::extract_tool_calls_canonical;

pub struct SequenceValidMetric;

/// Attach the per-rule record to a result.
///
/// Every return path carries it. Two of them used to not: the exact-sequence branches returned
/// early, so a suite setting both `sequence:` and `rules:` got `details = {}` and lost the record
/// this metric exists to produce. A helper rather than a fourth copy of the merge, because the
/// paths that forgot it were the two that were added last.
fn with_rule_record(mut result: MetricResult, record: &serde_json::Value) -> MetricResult {
    if let (Some(obj), Some(extra)) = (result.details.as_object_mut(), record.as_object()) {
        obj.extend(extra.clone());
    }
    result
}

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
        #[expect(
            clippy::wildcard_enum_match_arm,
            reason = "a metric declines every Expected variant but its own, and now says so with not_applicable() rather than a vacuous pass (#1949 layer 2)"
        )]
        let (policy_path, inline_sequence, inline_rules) = match expected {
            Expected::SequenceValid {
                policy,
                sequence,
                rules,
            } => (policy, sequence, rules),
            _ => return Ok(MetricResult::not_applicable()),
        };

        // 1. Resolve Rules & Sequence from Policy File (if any)
        let mut file_policy: Option<assay_core::model::Policy> = None;
        let (file_sequence, file_rules) = if let Some(path) = policy_path {
            if should_emit_deprecated_policy_warning(self.name(), path) {
                eprintln!(
                    "WARN: Deprecated policy file '{}' detected. Please migrate to inline usage.",
                    path
                );
                eprintln!(
                    "      To suppress this, set MCP_CONFIG_LEGACY=1 or run 'assay migrate'."
                );
            }

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
                // Keep the policy. An earlier version took `pol.sequences` and dropped the rest,
                // which discarded `aliases` -- so a rule naming an alias read the literal name,
                // matched nothing, and reported `held` on a trace the alias covers.
                let rules = pol.sequences.clone();
                file_policy = Some(pol);
                (None, Some(rules))
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
            return Ok(MetricResult::not_exercised(
                "no sequence and no rules configured",
            ));
        }

        // Parse Tool Calls
        let tool_calls: Vec<ToolCallRecord> = match extract_tool_calls_canonical(resp) {
            Ok(tool_calls) => tool_calls,
            Err(_) => {
                return Ok(MetricResult::fail(
                    0.0,
                    "sequence_valid could not read canonical tool-call evidence",
                ));
            }
        };

        // Sort by index
        let mut actual_sequence = tool_calls.clone();
        actual_sequence.sort_by_key(|k| k.index);
        let actual_names: Vec<String> = actual_sequence
            .iter()
            .map(|c| c.tool_name.clone())
            .collect();

        // 2. Validate Rules (DSL)
        //
        // Delegated to `assay_core::sequence_eval`, which is the only implementation of this
        // rule language. This metric used to carry its own, handling three of the eight
        // variants and resolving no aliases, so a `never_after` rule reported a clean run
        // instead of the violation it names. See that module for why one call replaced two
        // implementations rather than a parity test.
        let mut evaluations = Vec::new();
        if let Some(rules) = effective_rules {
            // The metric evaluates a finished run, so an unmet deadline is a violation rather
            // than a window still open. The proxy that also owns this language asks the other
            // question, which is why the extent is stated rather than assumed.
            evaluations = assay_core::sequence_eval::evaluate_rules(
                rules,
                &actual_names,
                file_policy.as_ref(),
                assay_core::sequence_eval::TraceExtent::Complete,
            );
        }
        let details = serde_json::json!({
            "rule_evaluations": evaluations
                .iter()
                .map(|e| serde_json::json!({
                    "rule_id": e.rule_id,
                    "kind": e.kind,
                    "outcome": e.outcome.label(),
                    "spanned": e.spanned,
                    "reason": e.reason,
                }))
                .collect::<Vec<_>>(),
        });

        if let Some(first) = evaluations.iter().find(|e| e.is_violation()) {
            let message = format!(
                "sequence_valid rule failed: {}",
                first.reason.as_deref().unwrap_or("constraint not met")
            );
            let mut result = MetricResult::fail(0.0, &message);
            if let (Some(obj), Some(extra)) = (result.details.as_object_mut(), details.as_object())
            {
                obj.extend(extra.clone());
            }
            return Ok(result);
        }

        // Every configured rule declined to decide. Reporting that as a pass would say the
        // policy held when nothing tested it, which is the vacuity this record exists to end.
        // Not gated on "no exact sequence configured". An earlier version was, which silenced
        // the signal for every suite setting both -- the common shape -- and the comment here
        // claimed the gate had been removed while the condition beneath it still carried it.
        // The exact-sequence check below still runs; this only reports that no rule decided.
        if !evaluations.is_empty()
            && evaluations
                .iter()
                .all(|e| e.outcome == assay_core::sequence_eval::RuleOutcome::NotExercised)
        {
            let mut result = MetricResult::not_exercised(
                "every configured sequence rule was vacuous for this trace",
            );
            if let (Some(obj), Some(extra)) = (result.details.as_object_mut(), details.as_object())
            {
                obj.extend(extra.clone());
            }
            return Ok(result);
        }

        // 3. Validate Exact Sequence (Legacy / Strict)
        if let Some(expected_sequence) = effective_sequence {
            if actual_names == *expected_sequence {
                return Ok(with_rule_record(MetricResult::pass(1.0), &details));
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
                return Ok(with_rule_record(
                    MetricResult::fail(
                        0.0,
                        &format!(
                            "sequence_valid mismatch. {}, (Expected {}: {:?}, Actual {}: {:?})",
                            diff_context,
                            expected_sequence.len(),
                            expected_sequence,
                            actual_names.len(),
                            actual_names
                        ),
                    ),
                    &details,
                ));
            }
        }

        Ok(with_rule_record(MetricResult::pass(1.0), &details))
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
    async fn empty_exact_sequence_requires_no_tool_calls() {
        let metric = SequenceValidMetric;
        let expected = Expected::SequenceValid {
            policy: None,
            sequence: Some(Vec::new()),
            rules: None,
        };

        let (empty_tc, empty_response) = make_test_case(Vec::new());
        let empty_result = metric
            .evaluate(&empty_tc, &expected, &empty_response)
            .await
            .unwrap();
        assert!(empty_result.passed);

        let (called_tc, called_response) = make_test_case(vec!["Search"]);
        let called_result = metric
            .evaluate(&called_tc, &expected, &called_response)
            .await
            .unwrap();
        assert!(!called_result.passed);
    }

    #[tokio::test]
    async fn empty_exact_sequence_rejects_malformed_present_tool_calls() {
        let metric = SequenceValidMetric;
        let expected = Expected::SequenceValid {
            policy: None,
            sequence: Some(Vec::new()),
            rules: None,
        };
        let (tc, mut resp) = make_test_case(Vec::new());
        resp.meta = serde_json::json!({"tool_calls": {"tool_name": "Search"}});

        let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
        assert!(!result.passed, "malformed-present evidence is not absence");
        assert_eq!(
            result.details["message"].as_str(),
            Some("sequence_valid could not read canonical tool-call evidence")
        );
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

    #[tokio::test]
    async fn malformed_tool_calls_fail_closed_before_rule_evaluation() {
        let metric = SequenceValidMetric;
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
        let resp = LlmResponse {
            meta: serde_json::json!({"tool_calls": {"tool_name": "A"}}),
            ..Default::default()
        };
        let expected = Expected::SequenceValid {
            policy: None,
            sequence: None,
            rules: Some(vec![SequenceRule::Require {
                tool: "A".to_string(),
            }]),
        };

        let result = metric.evaluate(&tc, &expected, &resp).await.unwrap();
        assert_eq!(result.score, 0.0);
        let msg = result.details["message"].as_str().unwrap();
        assert_eq!(
            msg,
            "sequence_valid could not read canonical tool-call evidence"
        );
    }
    /// End-to-end through the metric, not the evaluator: before delegation this rule kind
    /// fell through the `_` arm and the metric returned `pass(1.0)` on the exact trace the
    /// rule forbids. #2105's demonstration, run against `assay run`'s own path.
    #[tokio::test]
    async fn never_after_now_fails_the_metric_it_used_to_pass() {
        let (tc, resp) = make_test_case(vec!["list_dir", "read_credentials", "http_post"]);
        let expected = Expected::SequenceValid {
            policy: None,
            sequence: None,
            rules: Some(vec![SequenceRule::NeverAfter {
                trigger: "read_credentials".to_string(),
                forbidden: "http_post".to_string(),
            }]),
        };
        let result = SequenceValidMetric
            .evaluate(&tc, &expected, &resp)
            .await
            .unwrap();

        assert!(
            !result.passed,
            "never_after must fail on trigger-then-forbidden"
        );
        let evals = result.details["rule_evaluations"].as_array().unwrap();
        assert_eq!(evals.len(), 1);
        assert_eq!(evals[0]["outcome"], "violated");
        assert_eq!(
            evals[0]["rule_id"],
            "never_after:read_credentials->http_post"
        );
        assert_eq!(evals[0]["spanned"], serde_json::json!([1, 2]));
    }

    /// A policy whose every rule is vacuous is not a policy that held.
    #[tokio::test]
    async fn all_vacuous_rules_report_not_exercised() {
        let (tc, resp) = make_test_case(vec!["read"]);
        let expected = Expected::SequenceValid {
            policy: None,
            sequence: None,
            rules: Some(vec![SequenceRule::Before {
                first: "auth".to_string(),
                then: "write".to_string(),
            }]),
        };
        let result = SequenceValidMetric
            .evaluate(&tc, &expected, &resp)
            .await
            .unwrap();
        assert!(result.passed, "a vacuous rule is a status, never a failure");
        assert!(
            !result.is_exercised(),
            "but it must not read as an exercised pass"
        );
    }
    /// The record must survive every return path. Both exact-sequence branches returned early
    /// and dropped it, so a suite setting `sequence:` and `rules:` together got `details = {}`.
    #[tokio::test]
    async fn rule_evaluations_survive_the_exact_sequence_paths() {
        let rules = Some(vec![SequenceRule::Blocklist {
            pattern: "danger".to_string(),
        }]);

        // exact sequence matches
        let (tc, resp) = make_test_case(vec!["a"]);
        let expected = Expected::SequenceValid {
            policy: None,
            sequence: Some(vec!["a".to_string()]),
            rules: rules.clone(),
        };
        let ok = SequenceValidMetric
            .evaluate(&tc, &expected, &resp)
            .await
            .unwrap();
        assert!(ok.passed);
        assert!(
            ok.details.get("rule_evaluations").is_some(),
            "match path dropped the record"
        );

        // exact sequence mismatches
        let expected = Expected::SequenceValid {
            policy: None,
            sequence: Some(vec!["b".to_string()]),
            rules,
        };
        let bad = SequenceValidMetric
            .evaluate(&tc, &expected, &resp)
            .await
            .unwrap();
        assert!(!bad.passed);
        assert!(
            bad.details.get("rule_evaluations").is_some(),
            "mismatch path dropped the record"
        );
    }

    /// The metric evaluates a finished run, so an unmet `require` is decided. Passing `Partial`
    /// would report it as undecided and the suite would pass.
    #[tokio::test]
    async fn the_metric_evaluates_a_finished_run() {
        let (tc, resp) = make_test_case(vec!["b"]);
        let expected = Expected::SequenceValid {
            policy: None,
            sequence: None,
            rules: Some(vec![SequenceRule::Require {
                tool: "a".to_string(),
            }]),
        };
        let result = SequenceValidMetric
            .evaluate(&tc, &expected, &resp)
            .await
            .unwrap();
        assert!(
            !result.passed,
            "a finished run that never called 'a' violates require"
        );
    }

    /// The message a reader has matched on since before this metric delegated.
    #[tokio::test]
    async fn require_keeps_its_message() {
        let (tc, resp) = make_test_case(vec!["b"]);
        let expected = Expected::SequenceValid {
            policy: None,
            sequence: None,
            rules: Some(vec![SequenceRule::Require {
                tool: "a".to_string(),
            }]),
        };
        let result = SequenceValidMetric
            .evaluate(&tc, &expected, &resp)
            .await
            .unwrap();
        assert_eq!(
            result.details["message"].as_str().unwrap(),
            "sequence_valid rule failed: required tool 'a' not found in trace"
        );
    }

    /// The vacuity signal is not gated on the absence of an exact sequence. It was, which
    /// silenced it for every suite configuring both.
    #[tokio::test]
    async fn vacuity_is_reported_even_when_an_exact_sequence_is_configured() {
        let (tc, resp) = make_test_case(vec!["read"]);
        let expected = Expected::SequenceValid {
            policy: None,
            sequence: Some(vec!["read".to_string()]),
            rules: Some(vec![SequenceRule::Before {
                first: "auth".to_string(),
                then: "write".to_string(),
            }]),
        };
        let result = SequenceValidMetric
            .evaluate(&tc, &expected, &resp)
            .await
            .unwrap();
        // The record reads `not_exercised` whether or not the gate is present, so asserting on
        // it proves nothing about the gate. What the gate decides is the metric's own status.
        assert!(
            !result.is_exercised(),
            "a wholly vacuous ruleset must not read as an exercised pass merely because an \
             exact sequence is configured alongside it"
        );
        let evals = result.details["rule_evaluations"].as_array().unwrap();
        assert_eq!(evals[0]["outcome"], "not_exercised");
    }
}
