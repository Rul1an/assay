pub mod analyzers;
pub mod model;

use chrono::Utc;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use crate::config::path_resolver::PathResolver;
use crate::errors::diagnostic::{codes, Diagnostic};
use crate::model::{EvalConfig, Expected};
use crate::validate::{validate, ValidateOptions};

use model::*;

#[derive(Debug, Clone)]
pub struct DoctorOptions {
    pub config_path: PathBuf,
    pub trace_file: Option<PathBuf>,
    pub baseline_file: Option<PathBuf>,
    pub db_path: Option<PathBuf>,
    pub replay_strict: bool,
}

pub async fn doctor(
    cfg: &EvalConfig,
    opts: &DoctorOptions,
    resolver: &PathResolver,
) -> anyhow::Result<DoctorReport> {
    let mut notes = vec![];
    let mut diagnostics: Vec<Diagnostic> = vec![];

    let vopts = ValidateOptions {
        trace_file: opts.trace_file.clone(),
        baseline_file: opts.baseline_file.clone(),
        replay_strict: opts.replay_strict,
    };
    let vreport = validate(cfg, &vopts, resolver).await?;
    diagnostics.extend(vreport.diagnostics);

    // A test's `policy:` file is read once, by `validate` above, through the binding `assay run`
    // applies before the test runs, and a file it refuses is already in `diagnostics` with run's
    // reason. Doctor used to parse each file again as a sequence policy document whatever the test
    // kind, which refused files run accepts (an `args_valid` file in the `schemas:` form, either
    // list form of a `sequence_valid` file) and reported a second error beside run's for the rest.
    // That parse also fed `analyzers::policy::analyze_policy_usage`, whose alias check cannot fire
    // on a file run accepts: run refuses a sequence policy file that declares aliases or tools.

    analyzers::config::analyze_config_integrity(cfg, resolver, &mut diagnostics);

    if let Some(p) = &opts.trace_file {
        analyzers::trace::analyze_trace_schema(p, &mut diagnostics);
    }

    let config_summary = Some(summarize_config(cfg));

    let trace_summary = match &opts.trace_file {
        Some(p) => summarize_trace(p, cfg, &mut diagnostics).ok(),
        None => None,
    };

    let baseline_summary = match &opts.baseline_file {
        Some(p) => summarize_baseline(p, &mut diagnostics).ok(),
        None => None,
    };

    let db_summary = match &opts.db_path {
        Some(p) => summarize_db(p, &mut diagnostics).ok(),
        None => None,
    };

    let caches = summarize_caches(&mut notes);

    let suggested_actions = suggest_from(&diagnostics, cfg, &trace_summary, &baseline_summary);

    Ok(DoctorReport {
        schema_version: 1,
        generated_at: Utc::now().to_rfc3339(),
        assay_version: env!("CARGO_PKG_VERSION").to_string(),
        platform: PlatformInfo {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
        },
        inputs: DoctorInputs {
            config_path: opts.config_path.display().to_string(),
            trace_file: opts.trace_file.as_ref().map(|p| p.display().to_string()),
            baseline_file: opts.baseline_file.as_ref().map(|p| p.display().to_string()),
            db_path: opts.db_path.as_ref().map(|p| p.display().to_string()),
            replay_strict: opts.replay_strict,
        },
        config: config_summary,
        trace: trace_summary,
        baseline: baseline_summary,
        db: db_summary,
        caches,
        diagnostics,
        suggested_actions,
        notes,
    })
}

// ... include existing summarize helpers ...
// (Omitting full copy-paste of helpers to keep context small, assuming I can append them or they are preserved if I use smart edit, verify?)
// Since I'm replacing the whole file content essentially (or a large chunk), I need to be careful.
// I will use `replace_file_content` targeting the top section effectively.

fn summarize_config(cfg: &EvalConfig) -> ConfigSummary {
    use std::collections::BTreeMap;
    let mut metric_counts: BTreeMap<String, u32> = BTreeMap::new();

    for tc in &cfg.tests {
        let key = match &tc.expected {
            Expected::MustContain { .. } => "must_contain",
            Expected::MustNotContain { .. } => "must_not_contain",
            Expected::RegexMatch { .. } => "regex_match",
            Expected::RegexNotMatch { .. } => "regex_not_match",
            Expected::JsonSchema { .. } => "json_schema",
            Expected::SemanticSimilarityTo { .. } => "semantic_similarity_to",
            Expected::Faithfulness { .. } => "faithfulness",
            Expected::Relevance { .. } => "relevance",
            Expected::JudgeCriteria { .. } => "judge_criteria",
            Expected::ArgsValid { .. } => "args_valid",
            Expected::SequenceValid { .. } => "sequence_valid",
            Expected::ToolBlocklist { .. } => "tool_blocklist",
            Expected::ToolDescriptionIntegrity { .. } => "tool_description_integrity",
            Expected::ToolOutputValid { .. } => "tool_output_valid",
            Expected::ToolCollisionDetect { .. } => "tool_collision_detect",
            Expected::Reference { .. } => "reference",
        }
        .to_string();

        *metric_counts.entry(key).or_insert(0) += 1;
    }

    let (mode, max_drop, min_floor) = cfg
        .settings
        .thresholding
        .as_ref()
        .map(|t| (t.mode.clone(), t.max_drop, t.min_floor))
        .unwrap_or((None, None, None));

    ConfigSummary {
        suite: cfg.suite.clone(),
        model: cfg.model.clone(),
        test_count: cfg.tests.len() as u32,
        metric_counts,
        thresholding_mode: mode,
        max_drop,
        min_floor,
    }
}

fn summarize_trace(
    path: &Path,
    _cfg: &EvalConfig,
    _diags: &mut Vec<Diagnostic>,
) -> anyhow::Result<TraceSummary> {
    // Keep it cheap: count lines, peek first line for schema_version/meta shape.
    let md = std::fs::metadata(path).ok();
    let approx_size_bytes = md.map(|m| m.len());

    let f = std::fs::File::open(path)?;
    let rdr = std::io::BufReader::new(f);

    let mut entries: u64 = 0;
    let mut first_schema: Option<u32> = None;
    let mut has_assay_meta = false;

    // Coverage: best-effort scan until N lines (avoid huge files)
    let mut has_embeddings = false;
    let mut has_judge_faithfulness = false;
    let mut has_judge_relevance = false;

    for (i, line) in rdr.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        // Attempt to ignore non-JSON lines if possible, but assume JSONL
        entries += 1;

        if i == 0 {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                first_schema = v
                    .get("schema_version")
                    .and_then(|x| x.as_u64())
                    .map(|x| x as u32);
                if v.get("meta").and_then(|m| m.get("assay")).is_some() {
                    has_assay_meta = true;
                }
            }
        }

        // scan first 200 entries for assay meta coverage
        if i < 200 {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                if let Some(meta) = v.get("meta").and_then(|m| m.get("assay")) {
                    if meta.pointer("/embeddings").is_some() {
                        has_embeddings = true;
                    }
                    if meta.pointer("/judge/faithfulness").is_some() {
                        has_judge_faithfulness = true;
                    }
                    if meta.pointer("/judge/relevance").is_some() {
                        has_judge_relevance = true;
                    }
                }
            }
        } else if has_embeddings && has_judge_faithfulness && has_judge_relevance {
            // Found everything, mostly likely. But we want to count entries completely?
            // If file is huge, counting lines might be slow. But usually fast enough.
            // Let's iterate all to count.
        }
    }

    Ok(TraceSummary {
        path: path.display().to_string(),
        entries,
        schema_version: first_schema,
        has_assay_meta,
        coverage: TraceCoverage {
            has_embeddings,
            has_judge_faithfulness,
            has_judge_relevance,
        },
        approx_size_bytes,
    })
}

fn summarize_baseline(
    path: &Path,
    _diags: &mut Vec<Diagnostic>,
) -> anyhow::Result<BaselineSummary> {
    let b = crate::baseline::Baseline::load(path)?;
    Ok(BaselineSummary {
        path: path.display().to_string(),
        suite: b.suite.clone(),
        schema_version: b.schema_version,
        assay_version: Some(b.assay_version.clone()),
        entry_count: b.entries.len() as u32,
    })
}

fn summarize_db(path: &Path, _diags: &mut Vec<Diagnostic>) -> anyhow::Result<DbSummary> {
    let size_bytes = std::fs::metadata(path).ok().map(|m| m.len());
    let store = crate::storage::store::Store::open(path)?;
    store.init_schema()?; // ensure migrations

    // These queries are intentionally light
    let stats = store
        .stats_best_effort()
        .unwrap_or(crate::storage::store::StoreStats {
            runs: None,
            results: None,
            last_run_id: None,
            last_run_at: None,
            version: None,
        });

    Ok(DbSummary {
        path: path.display().to_string(),
        size_bytes,
        runs: stats.runs,
        results: stats.results,
        last_run_id: stats.last_run_id,
        last_run_started_at: stats.last_run_at,
    })
}

fn summarize_caches(notes: &mut Vec<String>) -> CacheSummary {
    // best effort: read HOME and check ~/.assay/*
    let home = std::env::var("HOME").ok();
    if home.is_none() {
        notes.push("HOME not set; cannot inspect ~/.assay caches".to_string());
        return CacheSummary::default();
    }
    let home = home.unwrap();
    let cache_dir = format!("{}/.assay/cache", home);
    let emb_dir = format!("{}/.assay/embeddings", home);

    CacheSummary {
        assay_cache_dir: Some(cache_dir.clone()),
        assay_embeddings_dir: Some(emb_dir.clone()),
        cache_size_bytes: dir_size_bytes(&cache_dir).ok(),
        embeddings_size_bytes: dir_size_bytes(&emb_dir).ok(),
    }
}

// Simple recursive directory size without external crates
fn dir_size_bytes(p: &str) -> anyhow::Result<u64> {
    let mut total = 0u64;
    let path = std::path::Path::new(p);
    if !path.exists() {
        return Ok(0);
    }

    if path.is_file() {
        return Ok(path.metadata()?.len());
    }

    let entries = std::fs::read_dir(path)?;
    for entry in entries {
        let entry = entry?;
        let ft = entry.file_type()?;
        if ft.is_file() {
            total += entry.metadata()?.len();
        } else if ft.is_dir() {
            // Heuristic: limit recursion depth or just do 1 level?
            // Standard recursion is fine for cache dirs (usually flat or few levels)
            // But let's be careful about symlinks/cycles (ignore symlinks)
            if !ft.is_symlink() {
                total += dir_size_bytes(entry.path().to_str().unwrap_or(""))?;
            }
        }
    }
    Ok(total)
}

fn suggest_from(
    diags: &[Diagnostic],
    _cfg: &EvalConfig,
    trace: &Option<TraceSummary>,
    _baseline: &Option<BaselineSummary>,
) -> Vec<SuggestedAction> {
    let mut out = vec![];

    if diags.iter().any(|d| d.code == codes::E_TRACE_MISS) {
        out.push(SuggestedAction {
            title: "Fix trace miss (prompt drift)".into(),
            relates_to: "failure_mode_1_trace_miss".into(),
            why: "Config prompts must match trace prompts exactly in replay/offline modes.".into(),
            steps: vec![
                "Run: assay trace verify --trace <trace.jsonl> --config <eval.yaml>".into(),
                "If prompts changed intentionally: re-ingest + precompute.".into(),
            ],
        });
    }

    if diags
        .iter()
        .any(|d| d.code == codes::E_REPLAY_STRICT_MISSING)
    {
        out.push(SuggestedAction {
            title: "Make trace strict-replay ready".into(),
            relates_to: "failure_mode_??_strict_replay_missing".into(),
            why: "In --replay-strict, missing embeddings/judge meta is a hard setup error.".into(),
            steps: vec![
                "Run: assay trace precompute-embeddings --trace <trace.jsonl> --output <trace_enriched.jsonl> ...".into(),
                "Run: assay trace precompute-judge --trace <trace_enriched.jsonl> --output <trace_enriched.jsonl> ...".into(),
            ],
        });
    }

    if diags.iter().any(|d| d.code == codes::E_BASE_MISMATCH) {
        out.push(SuggestedAction {
            title: "Regenerate or select correct baseline".into(),
            relates_to: "failure_mode_3_schema_version_drift".into(),
            why: "Baseline suite/schema must match config suite/schema.".into(),
            steps: vec![
                "Export on main: assay ci --config <eval.yaml> --trace-file <main.jsonl> --export-baseline baseline.json".into(),
                "Gate PR: assay ci --baseline baseline.json".into(),
            ],
        });
    }

    // Heuristic: large trace performance
    if let Some(t) = trace {
        if t.entries > 50_000 {
            out.push(SuggestedAction {
                title: "Speed up CI for large traces".into(),
                relates_to: "failure_mode_9_large_trace_performance".into(),
                why: "Large trace files increase parse time; CI should use a smaller slice + incremental.".into(),
                steps: vec![
                    "Use a CI slice trace (e.g. top 1k).".into(),
                    "Enable incremental: assay ci --incremental".into(),
                    "Use precompute + --replay-strict for offline CI.".into(),
                ],
            });
        }
    }

    out
}

#[cfg(test)]
mod policy_file_tests {
    use super::*;
    use crate::model::{Settings, TestCase, TestInput};

    fn cfg_with(expected: serde_json::Value) -> EvalConfig {
        EvalConfig {
            version: 1,
            suite: "s".into(),
            model: "trace".into(),
            settings: Settings::default(),
            thresholds: Default::default(),
            otel: Default::default(),
            tests: vec![TestCase {
                id: "t1".into(),
                input: TestInput {
                    prompt: "p".into(),
                    context: None,
                },
                expected: serde_json::from_value(expected).expect("expected block"),
                assertions: None,
                on_error: None,
                tags: vec![],
                metadata: None,
            }],
        }
    }

    async fn doctor_on(kind: &str, policy_source: &str) -> Vec<Diagnostic> {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("policy.yaml");
        std::fs::write(&path, policy_source).expect("write policy");
        let cfg = cfg_with(serde_json::json!({"type": kind, "policy": path.to_string_lossy()}));
        let opts = DoctorOptions {
            config_path: dir.path().join("eval.yaml"),
            trace_file: None,
            baseline_file: None,
            db_path: None,
            replay_strict: false,
        };
        let resolver = PathResolver::new(Path::new("eval.yaml"));
        doctor(&cfg, &opts, &resolver)
            .await
            .expect("doctor")
            .diagnostics
    }

    /// Every form of policy file `assay run` accepts, for the test kind that reads it. Doctor
    /// used to parse each file as a sequence policy document whatever the test, so it refused an
    /// `args_valid` file in the `schemas:` form and both list forms of a `sequence_valid` file.
    #[tokio::test]
    async fn accepts_every_policy_file_form_run_accepts() {
        let accepted = [
            (
                "args_valid",
                "version: \"2.0\"\nschemas:\n  exec:\n    type: object\n    required: [cmd]\n",
            ),
            ("sequence_valid", "- exec\n"),
            ("sequence_valid", "- type: require\n  tool: exec\n"),
            (
                "sequence_valid",
                "version: \"1.0\"\nsequences:\n  - { type: require, tool: exec }\n",
            ),
        ];
        for (kind, source) in accepted {
            let diagnostics = doctor_on(kind, source).await;
            let parse: Vec<_> = diagnostics
                .iter()
                .filter(|d| d.code == codes::E_CFG_PARSE)
                .collect();
            assert!(parse.is_empty(), "{kind} {source:?}: {parse:?}");
        }
    }

    /// A file `assay run` refuses is reported once, with run's reason, and that reason names a
    /// field the policy form does not have (`rules` is the inline spelling of `sequences`)
    /// rather than describing the document's shape.
    #[tokio::test]
    async fn reports_a_refused_policy_file_once_with_runs_reason() {
        let diagnostics = doctor_on(
            "sequence_valid",
            "version: \"1.0\"\nrules:\n  - { type: require, tool: exec }\n",
        )
        .await;
        let parse: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == codes::E_CFG_PARSE)
            .collect();
        assert_eq!(parse.len(), 1, "{parse:?}");
        assert!(parse[0].message.contains("(rules)"), "{:?}", parse[0]);
        assert!(
            parse[0].message.contains("reads only `sequences`"),
            "{:?}",
            parse[0]
        );
    }
}
