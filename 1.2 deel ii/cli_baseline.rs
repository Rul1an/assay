//! CLI commands for baseline management
//!
//! - `assay baseline save` - Save current coverage as baseline
//! - `assay baseline diff` - Compare current coverage against baseline

use clap::{Args, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Args, Debug)]
pub struct BaselineArgs {
    #[command(subcommand)]
    pub command: BaselineCommand,
}

#[derive(Subcommand, Debug)]
pub enum BaselineCommand {
    /// Save current coverage as a baseline
    Save(BaselineSaveArgs),
    
    /// Compare current coverage against a baseline
    Diff(BaselineDiffArgs),
    
    /// Show baseline info
    Show(BaselineShowArgs),
}

#[derive(Args, Debug)]
pub struct BaselineSaveArgs {
    /// Policy file
    #[arg(short, long)]
    pub policy: PathBuf,
    
    /// Traces file or directory
    #[arg(short, long)]
    pub traces: PathBuf,
    
    /// Output baseline file (default: .assay/baseline.yaml)
    #[arg(short, long, default_value = ".assay/baseline.yaml")]
    pub output: PathBuf,
    
    /// Git commit hash to record
    #[arg(long)]
    pub commit: Option<String>,
    
    /// Git branch name to record
    #[arg(long)]
    pub branch: Option<String>,
    
    /// Auto-detect git info from current repository
    #[arg(long, default_value = "true")]
    pub git_auto: bool,
}

#[derive(Args, Debug)]
pub struct BaselineDiffArgs {
    /// Policy file
    #[arg(short, long)]
    pub policy: PathBuf,
    
    /// Traces file or directory
    #[arg(short, long)]
    pub traces: PathBuf,
    
    /// Baseline file to compare against
    #[arg(short, long, default_value = ".assay/baseline.yaml")]
    pub baseline: PathBuf,
    
    /// Output format: terminal, json, github
    #[arg(short, long, default_value = "terminal")]
    pub format: String,
    
    /// Fail if coverage regressed
    #[arg(long, default_value = "true")]
    pub fail_on_regression: bool,
    
    /// Minimum coverage delta to consider regression (percentage points)
    #[arg(long, default_value = "1.0")]
    pub regression_threshold: f64,
}

#[derive(Args, Debug)]
pub struct BaselineShowArgs {
    /// Baseline file
    #[arg(short, long, default_value = ".assay/baseline.yaml")]
    pub baseline: PathBuf,
    
    /// Output format: terminal, json, yaml
    #[arg(short, long, default_value = "terminal")]
    pub format: String,
}

/// Execute baseline save command
pub fn execute_save(args: BaselineSaveArgs) -> ExitCode {
    // Load policy
    let policy = match load_policy(&args.policy) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error loading policy: {}", e);
            return ExitCode::from(2);
        }
    };
    
    // Load traces
    let traces = match load_traces(&args.traces) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Error loading traces: {}", e);
            return ExitCode::from(2);
        }
    };
    
    // Run coverage analysis
    let analyzer = assay_core::coverage::CoverageAnalyzer::from_policy(&policy);
    let report = analyzer.analyze(&traces, 0.0); // No threshold for baseline
    
    // Get git info
    let (commit, branch) = if args.git_auto {
        (
            args.commit.or_else(get_git_commit),
            args.branch.or_else(get_git_branch),
        )
    } else {
        (args.commit, args.branch)
    };
    
    // Create baseline
    let baseline = assay_core::baseline::Baseline::from_coverage_report(
        &report,
        &policy,
        commit,
        branch,
    );
    
    // Save baseline
    if let Err(e) = baseline.save(&args.output) {
        eprintln!("Error saving baseline: {}", e);
        return ExitCode::from(2);
    }
    
    println!("✅ Baseline saved to {}", args.output.display());
    println!("   Coverage: {:.1}%", report.overall_coverage_pct);
    println!("   Tools: {}/{}", 
        report.tool_coverage.tools_seen_in_traces,
        report.tool_coverage.total_tools_in_policy);
    println!("   Rules: {}/{}",
        report.rule_coverage.rules_triggered,
        report.rule_coverage.total_rules);
    
    if let Some(commit) = &baseline.commit {
        println!("   Commit: {}", commit);
    }
    
    ExitCode::SUCCESS
}

/// Execute baseline diff command
pub fn execute_diff(args: BaselineDiffArgs) -> ExitCode {
    // Load baseline
    let baseline = match assay_core::baseline::Baseline::from_file(&args.baseline) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Error loading baseline: {}", e);
            eprintln!("Hint: Run 'assay baseline save' first to create a baseline");
            return ExitCode::from(2);
        }
    };
    
    // Load policy
    let policy = match load_policy(&args.policy) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error loading policy: {}", e);
            return ExitCode::from(2);
        }
    };
    
    // Verify policy matches baseline
    if policy.name != baseline.policy_name {
        eprintln!("Warning: Policy name '{}' differs from baseline '{}'",
            policy.name, baseline.policy_name);
    }
    
    // Load traces
    let traces = match load_traces(&args.traces) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Error loading traces: {}", e);
            return ExitCode::from(2);
        }
    };
    
    // Run coverage analysis
    let analyzer = assay_core::coverage::CoverageAnalyzer::from_policy(&policy);
    let report = analyzer.analyze(&traces, 0.0);
    
    // Compare against baseline
    let diff = baseline.diff(&report);
    
    // Output based on format
    match args.format.as_str() {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&diff).unwrap());
        }
        "github" => {
            print!("{}", diff.to_github_annotation());
            // Also print summary
            println!("::notice::Coverage: {:.1}% (baseline: {:.1}%, Δ {:+.1}%)",
                diff.coverage_delta.current_pct,
                diff.coverage_delta.baseline_pct,
                diff.coverage_delta.delta_pct);
        }
        _ => {
            println!("{}", diff.to_terminal());
        }
    }
    
    // Exit code based on regression
    if args.fail_on_regression && diff.is_regression {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// Execute baseline show command
pub fn execute_show(args: BaselineShowArgs) -> ExitCode {
    let baseline = match assay_core::baseline::Baseline::from_file(&args.baseline) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Error loading baseline: {}", e);
            return ExitCode::from(2);
        }
    };
    
    match args.format.as_str() {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&baseline).unwrap());
        }
        "yaml" => {
            println!("{}", serde_yaml::to_string(&baseline).unwrap());
        }
        _ => {
            println!("Baseline: {}", args.baseline.display());
            println!("  Created: {}", baseline.created_at.format("%Y-%m-%d %H:%M:%S UTC"));
            if let Some(commit) = &baseline.commit {
                println!("  Commit: {}", commit);
            }
            if let Some(branch) = &baseline.branch {
                println!("  Branch: {}", branch);
            }
            println!("  Policy: {} (v{})", baseline.policy_name, baseline.policy_version);
            println!("  Coverage: {:.1}%", baseline.coverage.overall_pct);
            println!("  Tools: {}/{} ({:.1}%)",
                baseline.coverage.tool_coverage.seen,
                baseline.coverage.tool_coverage.total,
                baseline.coverage.tool_coverage.pct);
            println!("  Rules: {}/{} ({:.1}%)",
                baseline.coverage.rule_coverage.triggered,
                baseline.coverage.rule_coverage.total,
                baseline.coverage.rule_coverage.pct);
        }
    }
    
    ExitCode::SUCCESS
}

// Helper functions

fn load_policy(path: &PathBuf) -> Result<assay_core::model::Policy, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Cannot read {}: {}", path.display(), e))?;
    
    serde_yaml::from_str(&content)
        .map_err(|e| format!("Invalid policy YAML: {}", e))
}

fn load_traces(path: &PathBuf) -> Result<Vec<assay_core::coverage::TraceRecord>, String> {
    if path.is_dir() {
        // Load all .jsonl and .json files from directory
        let mut traces = Vec::new();
        
        for entry in std::fs::read_dir(path)
            .map_err(|e| format!("Cannot read directory: {}", e))?
        {
            let entry = entry.map_err(|e| e.to_string())?;
            let file_path = entry.path();
            
            if let Some(ext) = file_path.extension() {
                if ext == "jsonl" || ext == "json" {
                    let file_traces = load_trace_file(&file_path)?;
                    traces.extend(file_traces);
                }
            }
        }
        
        Ok(traces)
    } else {
        load_trace_file(path)
    }
}

fn load_trace_file(path: &PathBuf) -> Result<Vec<assay_core::coverage::TraceRecord>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Cannot read {}: {}", path.display(), e))?;
    
    // Try JSONL first (one JSON per line)
    let mut traces = Vec::new();
    let mut jsonl_failed = false;
    
    for (idx, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        
        match serde_json::from_str::<TraceInput>(line) {
            Ok(input) => {
                traces.push(assay_core::coverage::TraceRecord {
                    trace_id: input.id.unwrap_or_else(|| format!("trace_{}", idx)),
                    tools_called: input.tools,
                    rules_triggered: std::collections::HashSet::new(),
                });
            }
            Err(_) => {
                jsonl_failed = true;
                break;
            }
        }
    }
    
    if jsonl_failed {
        // Try as JSON array
        traces.clear();
        let inputs: Vec<TraceInput> = serde_json::from_str(&content)
            .map_err(|e| format!("Invalid trace format: {}", e))?;
        
        for (idx, input) in inputs.into_iter().enumerate() {
            traces.push(assay_core::coverage::TraceRecord {
                trace_id: input.id.unwrap_or_else(|| format!("trace_{}", idx)),
                tools_called: input.tools,
                rules_triggered: std::collections::HashSet::new(),
            });
        }
    }
    
    Ok(traces)
}

#[derive(serde::Deserialize)]
struct TraceInput {
    #[serde(alias = "tool_calls", alias = "tools_called")]
    tools: Vec<String>,
    id: Option<String>,
}

fn get_git_commit() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok().map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

fn get_git_branch() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok().map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;
    
    fn create_test_policy(dir: &TempDir) -> PathBuf {
        let path = dir.path().join("policy.yaml");
        let mut file = std::fs::File::create(&path).unwrap();
        write!(file, r#"
version: "1.1"
name: "test"
tools:
  allow:
    - Search
    - Create
    - Update
  deny:
    - Delete
sequences:
  - type: max_calls
    tool: Search
    max: 3
"#).unwrap();
        path
    }
    
    fn create_test_traces(dir: &TempDir) -> PathBuf {
        let path = dir.path().join("traces.jsonl");
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(file, r#"{{"tools": ["Search", "Create"]}}"#).unwrap();
        writeln!(file, r#"{{"tools": ["Search", "Update"]}}"#).unwrap();
        path
    }
    
    #[test]
    fn test_load_policy() {
        let dir = TempDir::new().unwrap();
        let policy_path = create_test_policy(&dir);
        
        let policy = load_policy(&policy_path).unwrap();
        assert_eq!(policy.name, "test");
    }
    
    #[test]
    fn test_load_traces_jsonl() {
        let dir = TempDir::new().unwrap();
        let traces_path = create_test_traces(&dir);
        
        let traces = load_traces(&traces_path).unwrap();
        assert_eq!(traces.len(), 2);
        assert!(traces[0].tools_called.contains(&"Search".to_string()));
    }
    
    #[test]
    fn test_load_traces_json_array() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("traces.json");
        let mut file = std::fs::File::create(&path).unwrap();
        write!(file, r#"[
            {{"tools": ["Search", "Create"]}},
            {{"tools": ["Update"]}}
        ]"#).unwrap();
        
        let traces = load_traces(&path).unwrap();
        assert_eq!(traces.len(), 2);
    }
    
    #[test]
    fn test_git_helpers() {
        // These may or may not work depending on environment
        // Just verify they don't panic
        let _ = get_git_commit();
        let _ = get_git_branch();
    }
}
