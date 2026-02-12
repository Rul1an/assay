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
