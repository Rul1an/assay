//! Shared argument types used across multiple commands.

use clap::ValueEnum;

#[derive(ValueEnum, Clone, Debug, Default, PartialEq)]
pub enum ValidateOutputFormat {
    #[default]
    Text,
    Json,
    Sarif,
}

#[derive(clap::ValueEnum, Clone, Debug, Default, PartialEq)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
}

#[derive(clap::Args, Clone)]
pub struct JudgeArgs {
    /// Enable or disable LLM-as-judge evaluation
    /// - none: judge calls disabled (replay/trace-only)
    /// - openai: live judge calls via OpenAI
    /// - fake: deterministic fake judge (tests/dev)
    #[arg(long, default_value = "none", env = "VERDICT_JUDGE")]
    pub judge: String,

    /// Alias for --judge none
    #[arg(long, conflicts_with = "judge")]
    pub no_judge: bool,

    /// Judge model identifier (provider-specific)
    /// Example: gpt-4o-mini
    #[arg(long, env = "VERDICT_JUDGE_MODEL")]
    pub judge_model: Option<String>,

    /// Number of judge samples per test (majority vote)
    /// Default: 3
    /// Tip: for critical production gates consider: --judge-samples 5
    #[arg(long, default_value_t = 3, env = "VERDICT_JUDGE_SAMPLES")]
    pub judge_samples: u32,

    /// Ignore judge cache and re-run judge calls (live mode only)
    #[arg(long)]
    pub judge_refresh: bool,

    /// Temperature used for judge calls (affects cache key)
    /// Default: 0.0
    #[arg(long, default_value_t = 0.0, env = "VERDICT_JUDGE_TEMPERATURE")]
    pub judge_temperature: f32,

    /// Max tokens for judge response (affects cache key)
    /// Default: 800
    #[arg(long, default_value_t = 800, env = "VERDICT_JUDGE_MAX_TOKENS")]
    pub judge_max_tokens: u32,

    /// Start with env (VERDICT_JUDGE_API_KEY could be supported but OPENAI_API_KEY is primary)
    #[arg(long, hide = true)]
    pub judge_api_key: Option<String>,
}

impl Default for JudgeArgs {
    fn default() -> Self {
        Self {
            judge: "none".to_string(),
            no_judge: false,
            judge_model: None,
            judge_samples: 3,
            judge_refresh: false,
            judge_temperature: 0.0,
            judge_max_tokens: 800,
            judge_api_key: None,
        }
    }
}

/// The `--format` vocabulary for the two MCP transcript importers.
///
/// A CLI-side enum rather than `assay_core`'s `McpInputFormat` directly, because that type lives in
/// a library that must not depend on clap, and because the accepted spellings include aliases
/// (`mcp-inspector`, `sse-legacy`) that `ValueEnum` has to declare for `--help` to list them.
///
/// Two spellings of one vocabulary is exactly the drift this repo keeps paying for, so the mapping
/// is total and `mcp_format_vocabularies_agree` holds it to `McpInputFormat::from_cli_label`. The
/// enum is the boundary; the core function stays the definition.
#[derive(clap::ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum McpTranscriptFormat {
    #[default]
    #[value(alias = "mcp-inspector", alias = "mcp-inspector@v1")]
    Inspector,
    Jsonrpc,
    #[value(name = "streamable-http")]
    StreamableHttp,
    #[value(name = "http-sse", alias = "sse-legacy")]
    HttpSse,
}

impl McpTranscriptFormat {
    /// The label `McpInputFormat::from_cli_label` understands.
    pub fn as_core_label(self) -> &'static str {
        match self {
            Self::Inspector => "inspector",
            Self::Jsonrpc => "jsonrpc",
            Self::StreamableHttp => "streamable-http",
            Self::HttpSse => "http-sse",
        }
    }

    pub fn to_core(self) -> assay_core::mcp::McpInputFormat {
        // `expect` rather than a fallback: `as_core_label` returns a literal this enum controls, so
        // a `None` here means the two vocabularies have diverged, which the parity test catches
        // first and which must not degrade into a silent default at runtime.
        assay_core::mcp::McpInputFormat::from_cli_label(self.as_core_label())
            .expect("McpTranscriptFormat label is not one McpInputFormat accepts")
    }
}

/// Output format for `policy generate`.
///
/// `generate.rs:108` passed a bare `String` to `serialize`, whose `_` arm wrote YAML. `--format
/// jsom` produced a YAML policy at exit 0, into a path the user may well have named `.json`.
#[derive(clap::ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PolicyOutputFormat {
    #[default]
    Yaml,
    Json,
}

/// Output format for `evidence show`.
#[derive(clap::ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShowFormat {
    #[default]
    Table,
    Json,
}

#[cfg(test)]
mod mcp_format_vocabulary_tests {
    use super::McpTranscriptFormat;
    use clap::ValueEnum;

    /// Every CLI spelling — variant names and aliases — is one `from_cli_label` accepts, and every
    /// variant maps to a distinct core format.
    ///
    /// Without this the two lists drift and the CLI advertises a value the core rejects, which is
    /// the failure `--format sse-legacy` would have hit the moment the alias was dropped.
    #[test]
    fn mcp_format_vocabularies_agree() {
        let mut seen = std::collections::BTreeSet::new();
        for variant in McpTranscriptFormat::value_variants() {
            let core = variant.to_core();
            assert!(
                seen.insert(format!("{core:?}")),
                "{variant:?} maps to a core format another variant already claims"
            );
        }
        assert_eq!(seen.len(), 4, "a core format lost its CLI spelling");

        // The aliases the old `String` argument accepted must still parse.
        for label in [
            "inspector",
            "mcp-inspector",
            "jsonrpc",
            "streamable-http",
            "http-sse",
            "sse-legacy",
        ] {
            assert!(
                McpTranscriptFormat::from_str(label, true).is_ok(),
                "`--format {label}` used to work and no longer parses"
            );
        }
    }
}
