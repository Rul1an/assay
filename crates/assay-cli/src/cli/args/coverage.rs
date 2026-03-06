use std::path::PathBuf;

#[derive(clap::Args, Debug, Clone)]
pub struct CoverageArgs {
    /// Path to JSONL tool/decision events for coverage_report_v1 generation.
    #[arg(long)]
    pub input: Option<std::path::PathBuf>,

    /// Output path for coverage_report_v1 JSON.
    #[arg(long)]
    pub out: Option<std::path::PathBuf>,

    /// Optional markdown output path for derived human-readable summary.
    #[arg(long = "out-md")]
    pub out_md: Option<std::path::PathBuf>,

    /// Tools declared by policy/config (repeatable).
    #[arg(long = "declared-tool")]
    pub declared_tools: Vec<String>,

    /// File with one declared tool per line (empty lines and # comments ignored).
    #[arg(long = "declared-tools-file")]
    pub declared_tools_file: Option<std::path::PathBuf>,

    #[arg(long, default_value = "eval.yaml")]
    pub config: std::path::PathBuf,

    #[arg(long)]
    pub policy: Option<PathBuf>,

    #[arg(long, alias = "traces")]
    pub trace_file: Option<std::path::PathBuf>,

    #[arg(long, default_value_t = 0.0)]
    pub min_coverage: f64,

    #[arg(long)]
    pub baseline: Option<PathBuf>,

    #[arg(long)]
    pub export_baseline: Option<PathBuf>,

    /// Output format.
    /// - `--format md|json` for `--input` mode
    /// - `--input` mode: json|md (text aliases to json; markdown/github alias to md)
    /// - legacy mode: text|json|markdown|github
    #[arg(long, default_value = "text")]
    pub format: String,

    /// Number of top routes to include in markdown output (default: 10).
    #[arg(long = "routes-top", default_value_t = 10)]
    pub routes_top: usize,
}
