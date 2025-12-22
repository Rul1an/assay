//! Trace verification with actionable diagnostics.
//!
//! Verifies that trace files cover all test prompts and provides
//! helpful hints when prompts don't match.

use crate::errors::{
    find_closest_match, ClosestMatch, Diagnostic, DiagnosticCode, DiagnosticContext,
    DiagnosticResult,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// A trace entry loaded from JSONL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEntry {
    /// The input prompt
    pub prompt: String,
    /// The model response
    pub response: String,
    /// Optional context chunks (for RAG)
    #[serde(default)]
    pub context: Vec<String>,
    /// Optional precomputed metadata
    #[serde(default)]
    pub meta: TraceMeta,
}

/// Precomputed metadata in a trace entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TraceMeta {
    /// Precomputed embeddings
    #[serde(default)]
    pub embeddings: Option<EmbeddingMeta>,
    /// Precomputed judge results
    #[serde(default)]
    pub judge: Option<JudgeMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingMeta {
    pub model: String,
    pub dimensions: usize,
    pub vector: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeMeta {
    pub model: String,
    pub metric: String,
    pub score: f64,
    pub samples: usize,
}

/// A test case from the configuration.
#[derive(Debug, Clone)]
pub struct TestCase {
    pub id: String,
    pub prompt: String,
    pub context: Vec<String>,
}

/// Result of trace verification.
#[derive(Debug, Clone, Serialize)]
pub struct VerifyResult {
    /// Total tests in config
    pub total_tests: usize,
    /// Tests with matching trace entries
    pub covered_tests: usize,
    /// Tests missing trace entries
    pub missing: Vec<MissingTest>,
    /// Whether strict replay is possible
    pub strict_replay_ready: StrictReplayStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissingTest {
    pub test_id: String,
    pub prompt: String,
    pub closest_match: Option<ClosestMatch>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StrictReplayStatus {
    pub embeddings_ready: bool,
    pub judge_ready: bool,
    pub missing_embeddings: Vec<String>,
    pub missing_judge: Vec<String>,
}

/// Trace verifier with configuration.
pub struct TraceVerifier {
    /// Minimum similarity score for closest match suggestions
    similarity_threshold: f64,
}

impl Default for TraceVerifier {
    fn default() -> Self {
        Self {
            similarity_threshold: 0.5,
        }
    }
}

impl TraceVerifier {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_similarity_threshold(mut self, threshold: f64) -> Self {
        self.similarity_threshold = threshold;
        self
    }

    /// Load trace entries from a JSONL file.
    pub fn load_traces(&self, path: &Path) -> DiagnosticResult<Vec<TraceEntry>> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            Diagnostic::new(
                DiagnosticCode::E002TraceFileNotFound,
                format!("Cannot read trace file: {}", path.display()),
                DiagnosticContext::ConfigError {
                    file_path: path.display().to_string(),
                    line: None,
                    column: None,
                    snippet: Some(e.to_string()),
                },
            )
        })?;

        let mut entries = Vec::new();
        for (line_num, line) in content.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }

            let entry: TraceEntry = serde_json::from_str(line).map_err(|e| {
                Diagnostic::new(
                    DiagnosticCode::E004TraceEntryMalformed,
                    format!("Invalid JSON at line {}", line_num + 1),
                    DiagnosticContext::ConfigError {
                        file_path: path.display().to_string(),
                        line: Some(line_num + 1),
                        column: None,
                        snippet: Some(format!("{}: {}", e, truncate(line, 50))),
                    },
                )
            })?;

            entries.push(entry);
        }

        Ok(entries)
    }

    /// Verify that all test prompts have matching trace entries.
    pub fn verify(
        &self,
        tests: &[TestCase],
        traces: &[TraceEntry],
        require_embeddings: bool,
        require_judge: bool,
    ) -> VerifyResult {
        // Build lookup map from prompt to trace entry
        let trace_prompts: Vec<String> = traces.iter().map(|t| t.prompt.clone()).collect();

        let mut covered_tests = 0;
        let mut missing = Vec::new();
        let mut missing_embeddings = Vec::new();
        let mut missing_judge = Vec::new();

        for test in tests {
            // Try exact match first
            let exact_match = traces.iter().find(|t| t.prompt == test.prompt);

            if let Some(trace) = exact_match {
                covered_tests += 1;

                // Check for precomputed data
                if require_embeddings && trace.meta.embeddings.is_none() {
                    missing_embeddings.push(test.id.clone());
                }
                if require_judge && trace.meta.judge.is_none() {
                    missing_judge.push(test.id.clone());
                }
            } else {
                // No exact match - find closest
                let closest = find_closest_match(
                    &test.prompt,
                    &trace_prompts,
                    self.similarity_threshold,
                );

                missing.push(MissingTest {
                    test_id: test.id.clone(),
                    prompt: test.prompt.clone(),
                    closest_match: closest,
                });
            }
        }

        let embeddings_ready = missing_embeddings.is_empty();
        let judge_ready = missing_judge.is_empty();

        VerifyResult {
            total_tests: tests.len(),
            covered_tests,
            missing,
            strict_replay_ready: StrictReplayStatus {
                embeddings_ready,
                judge_ready,
                missing_embeddings,
                missing_judge,
            },
        }
    }

    /// Verify and return diagnostics for any issues.
    pub fn verify_with_diagnostics(
        &self,
        tests: &[TestCase],
        traces: &[TraceEntry],
        require_embeddings: bool,
        require_judge: bool,
    ) -> Vec<Diagnostic> {
        let result = self.verify(tests, traces, require_embeddings, require_judge);
        let mut diagnostics = Vec::new();

        // Generate diagnostics for missing tests
        for missing in &result.missing {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::E001TraceMiss,
                format!("No matching trace entry for test '{}'", missing.test_id),
                DiagnosticContext::TraceMiss {
                    test_id: missing.test_id.clone(),
                    expected_prompt: missing.prompt.clone(),
                    closest_match: missing.closest_match.clone(),
                },
            ));
        }

        // Generate diagnostics for missing embeddings
        if require_embeddings && !result.strict_replay_ready.embeddings_ready {
            for test_id in &result.strict_replay_ready.missing_embeddings {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::E042EmbeddingsNotPrecomputed,
                    format!("Embeddings not precomputed for test '{}'", test_id),
                    DiagnosticContext::StrictReplayViolation {
                        test_id: test_id.clone(),
                        missing_data: vec!["embeddings".to_string()],
                    },
                ));
            }
        }

        // Generate diagnostics for missing judge
        if require_judge && !result.strict_replay_ready.judge_ready {
            for test_id in &result.strict_replay_ready.missing_judge {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::E060JudgeNotPrecomputed,
                    format!("Judge results not precomputed for test '{}'", test_id),
                    DiagnosticContext::StrictReplayViolation {
                        test_id: test_id.clone(),
                        missing_data: vec!["judge".to_string()],
                    },
                ));
            }
        }

        diagnostics
    }
}

/// Format verification result for terminal output.
pub fn format_verify_result(result: &VerifyResult, color: bool) -> String {
    let mut output = String::new();

    // Coverage summary
    let coverage_pct = if result.total_tests > 0 {
        (result.covered_tests as f64 / result.total_tests as f64) * 100.0
    } else {
        100.0
    };

    if color {
        if result.covered_tests == result.total_tests {
            output.push_str(&format!(
                "\x1b[32m✓\x1b[0m Coverage: {}/{} tests ({:.0}%)\n",
                result.covered_tests, result.total_tests, coverage_pct
            ));
        } else {
            output.push_str(&format!(
                "\x1b[31m✗\x1b[0m Coverage: {}/{} tests ({:.0}%)\n",
                result.covered_tests, result.total_tests, coverage_pct
            ));
        }
    } else {
        let symbol = if result.covered_tests == result.total_tests {
            "✓"
        } else {
            "✗"
        };
        output.push_str(&format!(
            "{} Coverage: {}/{} tests ({:.0}%)\n",
            symbol, result.covered_tests, result.total_tests, coverage_pct
        ));
    }

    // Missing tests
    if !result.missing.is_empty() {
        output.push_str("\nMissing trace entries:\n");
        for m in &result.missing {
            output.push_str(&format!("  - {} : \"{}\"\n", m.test_id, truncate(&m.prompt, 40)));
            if let Some(closest) = &m.closest_match {
                output.push_str(&format!(
                    "    Closest: \"{}\" (similarity: {:.2})\n",
                    truncate(&closest.prompt, 40),
                    closest.similarity
                ));
            }
        }
    }

    // Strict replay status
    let sr = &result.strict_replay_ready;
    if sr.embeddings_ready && sr.judge_ready {
        if color {
            output.push_str("\x1b[32m✓\x1b[0m Strict replay: ready\n");
        } else {
            output.push_str("✓ Strict replay: ready\n");
        }
    } else {
        if color {
            output.push_str("\x1b[33m⚠\x1b[0m Strict replay: not ready\n");
        } else {
            output.push_str("⚠ Strict replay: not ready\n");
        }

        if !sr.embeddings_ready {
            output.push_str(&format!(
                "  Missing embeddings: {}\n",
                sr.missing_embeddings.join(", ")
            ));
        }
        if !sr.judge_ready {
            output.push_str(&format!(
                "  Missing judge: {}\n",
                sr.missing_judge.join(", ")
            ));
        }
    }

    output
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test(id: &str, prompt: &str) -> TestCase {
        TestCase {
            id: id.to_string(),
            prompt: prompt.to_string(),
            context: vec![],
        }
    }

    fn make_trace(prompt: &str, response: &str) -> TraceEntry {
        TraceEntry {
            prompt: prompt.to_string(),
            response: response.to_string(),
            context: vec![],
            meta: TraceMeta::default(),
        }
    }

    #[test]
    fn test_verify_all_covered() {
        let tests = vec![
            make_test("t1", "Hello"),
            make_test("t2", "World"),
        ];
        let traces = vec![
            make_trace("Hello", "Hi"),
            make_trace("World", "Earth"),
        ];

        let verifier = TraceVerifier::new();
        let result = verifier.verify(&tests, &traces, false, false);

        assert_eq!(result.total_tests, 2);
        assert_eq!(result.covered_tests, 2);
        assert!(result.missing.is_empty());
    }

    #[test]
    fn test_verify_missing_with_closest_match() {
        let tests = vec![make_test("t1", "What is the capital of France?")];
        let traces = vec![make_trace("What is the capitol of France?", "Paris")];

        let verifier = TraceVerifier::new();
        let result = verifier.verify(&tests, &traces, false, false);

        assert_eq!(result.covered_tests, 0);
        assert_eq!(result.missing.len(), 1);

        let missing = &result.missing[0];
        assert!(missing.closest_match.is_some());

        let closest = missing.closest_match.as_ref().unwrap();
        assert!(closest.similarity > 0.9);
        assert!(closest.prompt.contains("capitol"));
    }

    #[test]
    fn test_verify_diagnostics() {
        let tests = vec![make_test("t1", "Hello world")];
        let traces = vec![make_trace("Hello wurld", "Response")];

        let verifier = TraceVerifier::new();
        let diagnostics = verifier.verify_with_diagnostics(&tests, &traces, false, false);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, DiagnosticCode::E001TraceMiss);

        let output = diagnostics[0].format_plain();
        assert!(output.contains("E001"));
        assert!(output.contains("t1"));
    }

    #[test]
    fn test_verify_strict_replay_missing_embeddings() {
        let tests = vec![make_test("t1", "Hello")];
        let traces = vec![make_trace("Hello", "Hi")];

        let verifier = TraceVerifier::new();
        let result = verifier.verify(&tests, &traces, true, false);

        assert_eq!(result.covered_tests, 1);
        assert!(!result.strict_replay_ready.embeddings_ready);
        assert!(result.strict_replay_ready.missing_embeddings.contains(&"t1".to_string()));
    }

    #[test]
    fn test_format_verify_result() {
        let result = VerifyResult {
            total_tests: 5,
            covered_tests: 3,
            missing: vec![
                MissingTest {
                    test_id: "t4".to_string(),
                    prompt: "Missing prompt 1".to_string(),
                    closest_match: None,
                },
                MissingTest {
                    test_id: "t5".to_string(),
                    prompt: "Missing prompt 2".to_string(),
                    closest_match: Some(ClosestMatch {
                        prompt: "Similar prompt 2".to_string(),
                        similarity: 0.85,
                        diff_positions: vec![],
                    }),
                },
            ],
            strict_replay_ready: StrictReplayStatus {
                embeddings_ready: true,
                judge_ready: true,
                missing_embeddings: vec![],
                missing_judge: vec![],
            },
        };

        let output = format_verify_result(&result, false);
        assert!(output.contains("3/5"));
        assert!(output.contains("t4"));
        assert!(output.contains("t5"));
        assert!(output.contains("0.85"));
    }
}
