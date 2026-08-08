use crate::on_error::ErrorPolicy;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvalConfig {
    #[serde(default, rename = "configVersion", alias = "version")]
    pub version: u32,
    pub suite: String,
    pub model: String,
    #[serde(
        default,
        skip_serializing_if = "crate::model::validation::is_default_settings"
    )]
    pub settings: Settings,
    #[serde(
        default,
        skip_serializing_if = "crate::model::validation::is_default_thresholds"
    )]
    pub thresholds: crate::thresholds::ThresholdConfig,
    #[serde(
        default,
        skip_serializing_if = "crate::model::validation::is_default_otel"
    )]
    pub otel: crate::config::otel::OtelConfig,
    pub tests: Vec<TestCase>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub judge: Option<JudgeConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thresholding: Option<ThresholdingSettings>,

    /// Global error handling policy (default: block)
    /// Can be overridden per-test
    #[serde(
        default,
        skip_serializing_if = "crate::model::validation::is_default_error_policy"
    )]
    pub on_error: ErrorPolicy,

    /// Bail on first failure (useful for CI)
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bail_on_first_failure: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ThresholdingSettings {
    pub mode: Option<String>,
    pub max_drop: Option<f64>,
    pub min_floor: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TestCase {
    pub id: String,
    pub input: TestInput,
    /// Skipped for the legacy empty-`must_contain` sentinel. The public model does
    /// not retain whether that shape came from omission or programmatic construction.
    #[serde(skip_serializing_if = "crate::model::validation::is_omitted_expected_sentinel")]
    pub expected: Expected,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assertions: Option<Vec<crate::agent_assertions::model::TraceAssertion>>,
    /// Per-test error handling policy override
    /// If None, uses settings.on_error
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_error: Option<ErrorPolicy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TestInput {
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "type")]
pub enum Expected {
    MustContain {
        #[serde(default)]
        must_contain: Vec<String>,
    },
    MustNotContain {
        #[serde(default)]
        must_not_contain: Vec<String>,
    },

    RegexMatch {
        pattern: String,
        #[serde(default)]
        flags: Vec<String>,
    },
    RegexNotMatch {
        pattern: String,
        #[serde(default)]
        flags: Vec<String>,
    },

    JsonSchema {
        json_schema: String,
        #[serde(default)]
        schema_file: Option<String>,
    },
    SemanticSimilarityTo {
        // canonical field
        #[serde(alias = "text")]
        semantic_similarity_to: String,

        // canonical field
        #[serde(
            default = "crate::model::validation::default_min_score",
            alias = "threshold"
        )]
        min_score: f64,

        #[serde(default)]
        thresholding: Option<ThresholdingConfig>,
    },
    JudgeCriteria {
        judge_criteria: serde_json::Value,
    },
    Faithfulness {
        #[serde(default = "crate::model::validation::default_min_score")]
        min_score: f64,
        rubric_version: Option<String>,
        #[serde(default)]
        thresholding: Option<ThresholdingConfig>,
    },
    Relevance {
        #[serde(default = "crate::model::validation::default_min_score")]
        min_score: f64,
        rubric_version: Option<String>,
        #[serde(default)]
        thresholding: Option<ThresholdingConfig>,
    },

    ArgsValid {
        #[serde(skip_serializing_if = "Option::is_none")]
        policy: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        schema: Option<serde_json::Value>,
    },
    SequenceValid {
        #[serde(skip_serializing_if = "Option::is_none")]
        policy: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sequence: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rules: Option<Vec<SequenceRule>>,
    },
    ToolBlocklist {
        blocked: Vec<String>,
    },
    /// Detect rug-pull attacks: verify tool descriptions/schemas match pinned expectations
    /// or remain consistent across multiple tool-list snapshots in the same trace.
    ToolDescriptionIntegrity {
        /// Pin specific tool definitions. If empty, snapshot-based mutation detection is used.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pinned_tools: Vec<PinnedTool>,
    },
    /// Validate tool call outputs against per-tool JSON schemas.
    ToolOutputValid {
        /// Map of tool_name → JSON Schema for the output. Only tools with a schema are checked.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        schemas: Option<serde_json::Value>,
    },
    /// Detect tool shadowing: same tool name registered by multiple servers.
    ToolCollisionDetect {
        /// Only flag collisions involving servers outside this list.
        /// Empty = flag all duplicate tool names regardless of server.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        trusted_servers: Vec<String>,
    },
    // For migration/legacy support
    #[serde(rename = "$ref")]
    Reference {
        path: String,
    },
}

/// A pinned tool definition for `ToolDescriptionIntegrity`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinnedTool {
    /// Tool name to match.
    pub name: String,
    /// Expected description (exact string match).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Expected SHA-256 hex of the canonical JSON-serialized `input_schema`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_sha256: Option<String>,
}

impl Default for Expected {
    /// NOTE: this default is **vacuous** — an empty `must_contain` makes the
    /// `must_contain` metric pass unconditionally, because it has no substring to
    /// look for. It exists only so `TestCase` can derive `Default` and so a test
    /// whose checks live in `assertions:` can omit `expected:` entirely.
    ///
    /// It must never be used as a parse fallback: an `expected:` block that fails
    /// to parse is a hard config error (see `model::serde`), and a test that ends
    /// up holding this value with no assertions is reported by the
    /// `W_CFG_VACUOUS_EXPECTED` rule in `assay validate`.
    fn default() -> Self {
        Expected::MustContain {
            must_contain: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub version: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub tools: ToolsPolicy,
    #[serde(default)]
    pub sequences: Vec<SequenceRule>,
    #[serde(default)]
    pub aliases: std::collections::HashMap<String, Vec<String>>,
    #[serde(default)]
    pub on_error: ErrorPolicy,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolsPolicy {
    #[serde(default)]
    pub allow: Option<Vec<String>>,
    #[serde(default)]
    pub deny: Option<Vec<String>>,
    #[serde(default)]
    pub require_args: Option<std::collections::HashMap<String, Vec<String>>>,
    #[serde(default)]
    pub arg_constraints: Option<
        std::collections::HashMap<String, std::collections::HashMap<String, serde_json::Value>>,
    >,
}

/// Which calls in a trace a rule step refers to.
///
/// Sequence rules used to name a tool and nothing else, so the correlation class that motivated
/// ADR-047 could not be written: "credential read followed by egress" is not a statement about two
/// tool names, it is a statement about two calls, one of which is identified by what it was given
/// (#2124). Both halves of that pair are ordinary `bash` in the trace that prompted it.
///
/// Untagged, so a bare string keeps meaning exactly what it meant before, including alias
/// resolution through the policy. Every existing config parses unchanged; the object form is the
/// new capability rather than a migration.
///
/// ```yaml
/// - type: never_after
///   trigger: { tool: bash, args_match: { command: "\\.aws/credentials" } }
///   forbidden: { tool: bash, args_match: { command: "^curl .*-d" } }
/// ```
///
/// `args_match` is a conjunction: every named argument must be present and its value must match
/// the regex. Values are matched against their JSON rendering, so a non-string argument is
/// matchable without a separate syntax, and a missing argument fails the match rather than being
/// skipped, because a rule that silently stops constraining is the failure this whole area exists
/// to prevent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum CallSelector {
    /// A tool name, resolved through policy aliases. The shape every pre-5.0.0 config uses.
    Tool(String),
    /// A tool name plus a constraint on the call's arguments.
    Matching {
        tool: String,
        args_match: std::collections::BTreeMap<String, String>,
    },
}

impl CallSelector {
    /// The tool name this selector names, for alias resolution and diagnostics.
    pub fn tool(&self) -> &str {
        match self {
            CallSelector::Tool(t) => t,
            CallSelector::Matching { tool, .. } => tool,
        }
    }

    /// The argument constraints, empty for a bare tool name.
    pub fn args_match(&self) -> Option<&std::collections::BTreeMap<String, String>> {
        match self {
            CallSelector::Tool(_) => None,
            CallSelector::Matching { args_match, .. } => Some(args_match),
        }
    }
}

impl std::fmt::Display for CallSelector {
    /// Stable rule ids: a bare tool renders as itself, so ids of existing rules do not change.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallSelector::Tool(t) => write!(f, "{t}"),
            CallSelector::Matching { tool, args_match } => {
                write!(f, "{tool}[")?;
                for (i, k) in args_match.keys().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "{k}")?;
                }
                write!(f, "]")
            }
        }
    }
}

impl From<&str> for CallSelector {
    fn from(s: &str) -> Self {
        CallSelector::Tool(s.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SequenceRule {
    Require {
        tool: CallSelector,
    },
    Eventually {
        tool: CallSelector,
        within: u32,
    },
    MaxCalls {
        tool: CallSelector,
        max: u32,
    },
    Before {
        first: CallSelector,
        then: CallSelector,
    },
    After {
        trigger: CallSelector,
        then: CallSelector,
        #[serde(default = "crate::model::validation::default_one")]
        within: u32,
    },
    NeverAfter {
        trigger: CallSelector,
        forbidden: CallSelector,
    },
    Sequence {
        tools: Vec<CallSelector>,
        #[serde(default)]
        strict: bool,
    },
    /// A substring match on the tool name. Deliberately not a selector: this rule is about names
    /// as text, and an argument constraint on a substring pattern would be two ideas in one field.
    Blocklist {
        pattern: String,
    },
}

// Helper for alias resolution
impl Policy {
    pub fn load<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let policy: Policy = serde_yaml::from_str(&content)?;
        Ok(policy)
    }

    pub fn resolve_alias(&self, tool_name: &str) -> Vec<String> {
        if let Some(members) = self.aliases.get(tool_name) {
            members.clone()
        } else {
            // If not an alias, return strict singleton if no alias found?
            // RFC says: "Matches SearchKnowledgeBase OR SearchWeb".
            // "Alias can be used anywhere a tool name is expected".
            // If we rely on resolve_alias to return all matches for a "rule target",
            // AND we want to support literals:
            // If 'Search' is in aliases, satisfy if match any alias member.
            // If 'Search' is NOT in aliases, it's a literal.
            vec![tool_name.to_string()]
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub id: String,
    pub tool_name: String,
    pub args: serde_json::Value,
    pub result: Option<serde_json::Value>,
    pub error: Option<serde_json::Value>,
    pub index: usize,
    pub ts_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ThresholdingConfig {
    pub max_drop: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct JudgeConfig {
    pub rubric_version: Option<String>,
    pub samples: Option<u32>,
    #[serde(default)]
    pub reliability: crate::judge::reliability::ReliabilityConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LlmResponse {
    pub text: String,
    pub provider: String,
    pub model: String,
    pub cached: bool,
    #[serde(default)]
    pub meta: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TestStatus {
    Pass,
    Fail,
    Flaky,
    Warn,
    Error,
    Skipped,
    Unstable,
    /// Action was allowed despite an upstream error (fail-open mode).
    AllowedOnError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResultRow {
    pub test_id: String,
    pub status: TestStatus,
    pub score: Option<f64>,
    pub cached: bool,
    pub message: String,
    #[serde(default)]
    pub details: serde_json::Value,
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub fingerprint: Option<String>,
    #[serde(default)]
    pub skip_reason: Option<String>,
    #[serde(default)]
    pub attempts: Option<Vec<AttemptRow>>,
    /// Error policy that was applied (if error occurred)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_policy_applied: Option<ErrorPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptRow {
    pub attempt_no: u32,
    pub status: TestStatus,
    pub message: String,
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub details: serde_json::Value,
}
