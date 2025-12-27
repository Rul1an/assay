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

    // 1) Validate (reuses PR-3.4.2)
    let vopts = ValidateOptions {
        trace_file: opts.trace_file.clone(),
        baseline_file: opts.baseline_file.clone(),
        replay_strict: opts.replay_strict,
    };
    let vreport = validate(cfg, &vopts, resolver).await?;
    diagnostics.extend(vreport.diagnostics);

    // 2) Config summary
    let config_summary = Some(summarize_config(cfg));

    // 3) Trace summary (best-effort)
    let trace_summary = match &opts.trace_file {
        Some(p) => summarize_trace(p, cfg, &mut diagnostics).ok(),
        None => None,
    };

    // 4) Baseline summary (best-effort)
    let baseline_summary = match &opts.baseline_file {
        Some(p) => summarize_baseline(p, &mut diagnostics).ok(),
        None => None,
    };

    // 5) DB summary (best-effort)
    let db_summary = match &opts.db_path {
        Some(p) => summarize_db(p, &mut diagnostics).ok(),
        None => None,
    };

    // 6) Cache summary (best-effort)
    let caches = summarize_caches(&mut notes);

    // 7) Suggested actions (Top-10 mapping)
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

    // Top-10 mapping by diagnostic code
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
