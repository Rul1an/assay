use std::path::PathBuf;

/// The spellings `coverage --format` accepts.
///
/// This is the argument's surface, not an output kind. `coverage` is two commands behind one
/// name, and the two honour different sets, so each mode narrows this to its own output type at
/// the boundary rather than matching on the spelling. Without that split a single enum over the
/// union would move the ambiguity into the type and make it look settled.
///
/// `github` is an alias of `markdown` because both modes already render markdown for it, so
/// collapsing the spelling changes nothing. `md` keeps a variant of its own for the opposite
/// reason: legacy mode has to refuse it by name, and an alias does not survive parsing.
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CoverageFormat {
    #[default]
    Text,
    Json,
    // No doc comment on a variant: clap renders per-value help as a bulleted list and drops the
    // inline `[possible values: ...]`, which would make this the one format argument in the CLI
    // whose help is laid out differently. The reasoning lives on the enum instead.
    Md,
    #[value(alias = "github")]
    Markdown,
}

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

    // Deliberately without an inline default, and the rationale is a plain comment because clap
    // renders a doc comment into `--help` and this is not something a user needs to read.
    //
    // `coverage` is two commands behind one name, and the modes do not share an output kind:
    // legacy has a text output and `--input` mode does not. One shared `default_value = "text"`
    // forced `--input` mode to accept `text` and mean JSON by it, which it did at exit 0 with no
    // note that the spelling was not honoured. The default belongs to the mode, so each mode
    // applies its own in `cmd_coverage`, next to the narrowing it already does.
    /// Output format. With `--input`: `json` (the default) or `md`. Without it: `text` (the
    /// default), `json` or `markdown`. Each mode rejects by name the spelling it cannot honour.
    /// `github` is accepted as an alias of `markdown`.
    #[arg(long)]
    pub format: Option<CoverageFormat>,

    /// Number of top routes to include in markdown output (default: 10).
    #[arg(long = "routes-top", default_value_t = 10)]
    pub routes_top: usize,
}
