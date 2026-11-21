use crate::on_error::ErrorPolicy;
use serde::{Deserialize, Serialize};

// The converter also reads versionless documents and older assertions. Keep the common
// fields in one declaration while giving each decoder its own version rule and case type.
macro_rules! eval_document {
    ($(#[derive($extra:path)])? $vis:vis $name:ident, $case:ty $(, #[$version_default:ident])?) => {
        assay_common::closed! {
            $(#[derive($extra)])?
            strict legacy_positional $vis struct $name {
                $(#[$version_default])?
                #[key = "configVersion", alias = "version"]
                version: u32,
                suite: String,
                model: String,
                #[default]
                #[omit_if = "crate::model::validation::is_default_settings"]
                settings: Settings,
                #[default]
                #[omit_if = "crate::model::validation::is_default_thresholds"]
                thresholds: crate::thresholds::ThresholdConfig,
                #[default]
                #[omit_if = "crate::model::validation::is_default_otel"]
                otel: crate::config::otel::OtelConfig,
                tests: Vec<$case>,
            }
        }
    };
}

eval_document!(#[derive(Serialize)] pub EvalConfig, TestCase);
eval_document!(pub(crate) MigrationEvalConfig, super::serde::MigrationTestCase, #[default]);

impl assay_common::doc::ClosedDoc for EvalConfig {}
impl assay_common::doc::ClosedDoc for MigrationEvalConfig {}

assay_common::closed! {
    #[derive(Default, Serialize)]
    pub struct Settings {
        #[omit_none]
        parallel: Option<usize>,
        #[omit_none]
        timeout_seconds: Option<u64>,
        #[omit_none]
        cache: Option<bool>,
        #[omit_none]
        seed: Option<u64>,
        #[omit_none]
        judge: Option<JudgeConfig>,
        #[omit_none]
        thresholding: Option<ThresholdingSettings>,

        /// Global error handling policy (default: block)
        /// Can be overridden per-test
        #[default]
        #[omit_if = "crate::model::validation::is_default_error_policy"]
        on_error: ErrorPolicy,

        /// Bail on first failure (useful for CI)
        #[default]
        #[omit_if = "std::ops::Not::not"]
        bail_on_first_failure: bool,
    }
}

assay_common::closed! {
    #[derive(Default, Serialize)]
    pub struct ThresholdingSettings {
        mode: Option<String>,
        max_drop: Option<f64>,
        min_floor: Option<f64>,
    }
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
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

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct TestInput {
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<Vec<String>>,
}

assay_common::closed! {
    pub enum Expected tagged "type" {
        "must_contain" => MustContain {
            #[default]
            must_contain: Vec<String>,
        },
        "must_not_contain" => MustNotContain {
            #[default]
            must_not_contain: Vec<String>,
        },

        "regex_match" => RegexMatch {
            pattern: String,
            #[default]
            flags: Vec<String>,
        },
        "regex_not_match" => RegexNotMatch {
            pattern: String,
            #[default]
            flags: Vec<String>,
        },

        "json_schema" => JsonSchema {
            json_schema: String,
            #[default]
            schema_file: Option<String>,
        },
        "semantic_similarity_to" => SemanticSimilarityTo {
            #[key = "semantic_similarity_to", alias = "text"]
            semantic_similarity_to: String,

            #[default = "crate::model::validation::default_min_score"]
            #[key = "min_score", alias = "threshold"]
            min_score: f64,

            #[default]
            #[contextual_option = "thresholding"]
            thresholding: Option<ThresholdingConfig>,
        },
        "judge_criteria" => JudgeCriteria {
            #[native_json = "judge_criteria"]
            judge_criteria: serde_json::Value,
        },
        "faithfulness" => Faithfulness {
            #[default = "crate::model::validation::default_min_score"]
            min_score: f64,
            rubric_version: Option<String>,
            #[default]
            #[contextual_option = "thresholding"]
            thresholding: Option<ThresholdingConfig>,
        },
        "relevance" => Relevance {
            #[default = "crate::model::validation::default_min_score"]
            min_score: f64,
            rubric_version: Option<String>,
            #[default]
            #[contextual_option = "thresholding"]
            thresholding: Option<ThresholdingConfig>,
        },

        "args_valid" => ArgsValid {
            #[omit_if = "Option::is_none"]
            policy: Option<String>,
            #[default]
            #[native_json = "schema"]
            #[omit_if = "Option::is_none"]
            schema: Option<serde_json::Value>,
        },
        "sequence_valid" => SequenceValid {
            #[omit_if = "Option::is_none"]
            policy: Option<String>,
            #[default]
            #[omit_if = "Option::is_none"]
            sequence: Option<Vec<String>>,
            #[default]
            #[omit_if = "Option::is_none"]
            rules: Option<Vec<SequenceRule>>,
        },
        "tool_blocklist" => ToolBlocklist {
            blocked: Vec<String>,
        },
        /// Detect rug-pull attacks: verify tool descriptions/schemas match pinned expectations
        /// or remain consistent across multiple tool-list snapshots in the same trace.
        "tool_description_integrity" => ToolDescriptionIntegrity {
            /// Pin specific tool definitions. If empty, snapshot-based mutation detection is used.
            #[default]
            #[omit_if = "Vec::is_empty"]
            pinned_tools: Vec<PinnedTool>,
        },
        /// Validate tool call outputs against per-tool JSON schemas.
        "tool_output_valid" => ToolOutputValid {
            /// Map of tool_name → JSON Schema for the output. Only tools with a schema are checked.
            #[default]
            #[native_json = "schemas"]
            #[omit_if = "Option::is_none"]
            schemas: Option<serde_json::Value>,
        },
        /// Detect tool shadowing: same tool name registered by multiple servers.
        "tool_collision_detect" => ToolCollisionDetect {
            /// Only flag collisions involving servers outside this list.
            /// Empty = flag all duplicate tool names regardless of server.
            #[default]
            #[omit_if = "Vec::is_empty"]
            trusted_servers: Vec<String>,
        },
        // For migration/legacy support
        "$ref" => Reference {
            path: String,
        },
    }
}

assay_common::closed! {
    /// A pinned tool definition for `ToolDescriptionIntegrity`.
    ///
    /// Public Serde still reads a key this type has no field for and leaves it out; the shared
    /// reader names it.
    #[derive(Serialize)]
    legacy_positional pub struct PinnedTool {
        /// Tool name to match.
        name: String,
        /// Expected description (exact string match).
        #[default]
        #[omit_none]
        description: Option<String>,
        /// Expected SHA-256 hex of the canonical JSON-serialized `input_schema`.
        #[default]
        #[omit_none]
        schema_sha256: Option<String>,
    }
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

assay_common::closed! {
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
    ///
    /// A map takes `tool` and `args_match` and nothing else: any other key refuses the selector by
    /// name. A second, misspelt `args_match` beside the first used to be dropped, and with it the
    /// constraint it carried.
    pub enum CallSelector name_or {
        /// A tool name, resolved through policy aliases. The shape every pre-5.0.0 config uses.
        Tool(String),
        /// A tool name plus a constraint on the call's arguments.
        Matching {
            tool: String,
            args_match: std::collections::BTreeMap<String, String>,
        },
    }
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

assay_common::closed! {
    /// One rule of a sequence check. A key the rule's type does not take refuses the rule by name:
    /// serde used to drop it, so `strict` misspelt on a `sequence` rule left the rule non-strict.
    pub enum SequenceRule tagged "type" {
        "require" => Require {
            tool: CallSelector,
        },
        "eventually" => Eventually {
            tool: CallSelector,
            within: u32,
        },
        "max_calls" => MaxCalls {
            tool: CallSelector,
            max: u32,
        },
        "before" => Before {
            first: CallSelector,
            then: CallSelector,
        },
        "after" => After {
            trigger: CallSelector,
            then: CallSelector,
            #[default = "crate::model::validation::default_one"]
            within: u32,
        },
        "never_after" => NeverAfter {
            trigger: CallSelector,
            forbidden: CallSelector,
        },
        "sequence" => Sequence {
            tools: Vec<CallSelector>,
            #[default]
            strict: bool,
        },
        /// A substring match on the tool name. Deliberately not a selector: this rule is about names
        /// as text, and an argument constraint on a substring pattern would be two ideas in one field.
        "blocklist" => Blocklist {
            pattern: String,
        },
    }
}

// Helper for alias resolution
impl Policy {
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

assay_common::closed! {
    #[derive(Default, Serialize)]
    strict legacy_positional pub struct ThresholdingConfig {
        max_drop: Option<f64>,
    }
}

impl assay_common::doc::FieldErrorContext for ThresholdingConfig {
    const CONTEXT: &'static str = "expected.thresholding";
}

assay_common::closed! {
    #[derive(Default, Serialize)]
    pub struct JudgeConfig {
        rubric_version: Option<String>,
        samples: Option<u32>,
        #[default]
        reliability: crate::judge::reliability::ReliabilityConfig,
    }
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
