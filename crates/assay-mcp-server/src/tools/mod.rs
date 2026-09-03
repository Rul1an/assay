use serde_json::Value;
use std::path::PathBuf;

use crate::cache::PolicyCaches;
use crate::config::ServerConfig;

pub mod call_tool_request;
pub use call_tool_request::{
    classify_call_tool_params, is_known_tool, CallToolDispatch, CallToolProtocolFault,
};

pub struct ToolContext {
    pub policy_root: PathBuf,
    pub policy_root_canon: PathBuf,
    pub cfg: ServerConfig,
    pub caches: PolicyCaches,
}

impl ToolContext {
    /// Securely resolves a user-provided path against the policy root.
    pub async fn resolve_policy_path(
        &self,
        user_path: &str,
    ) -> std::result::Result<PathBuf, ToolError> {
        // Delegate to pure function
        crate::security::resolve_policy_path(&self.policy_root_canon, user_path)
    }
}

const MAX_PUBLIC_MESSAGE_BYTES: usize = 4096;

/// UTF-8-safe prefix of `message` at most `MAX_PUBLIC_MESSAGE_BYTES` bytes.
/// Returns the full slice when it fits; truncates on a char boundary otherwise.
/// No suffix is added beyond the ceiling.
fn bound_public_message(message: &str) -> &str {
    if message.len() <= MAX_PUBLIC_MESSAGE_BYTES {
        return message;
    }
    let mut end = MAX_PUBLIC_MESSAGE_BYTES;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    &message[..end]
}

pub struct ToolError {
    pub code: String,
    pub message: String,
    pub details: Option<Value>,
}

impl serde::Serialize for ToolError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(serde::Serialize)]
        struct BoundedView<'a> {
            code: &'a str,
            message: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            details: &'a Option<Value>,
        }
        let view = BoundedView {
            code: &self.code,
            message: bound_public_message(&self.message),
            details: &self.details,
        };
        view.serialize(serializer)
    }
}

/// The three approved fixed parse summaries.
#[derive(Debug, Clone, Copy)]
pub(crate) enum PolicyParseFailure {
    YamlSyntax,
    RootNotMapping,
    Structure,
}

impl PolicyParseFailure {
    fn summary(self) -> &'static str {
        match self {
            Self::YamlSyntax => "Policy YAML is invalid",
            Self::RootNotMapping => "Policy root must be a mapping",
            Self::Structure => "Policy structure is invalid",
        }
    }
}

impl ToolError {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.to_string(),
            message: bound_public_message(message).to_owned(),
            details: None,
        }
    }

    /// Construct a parse error with a fixed summary and optional numeric location.
    pub(crate) fn policy_parse(
        class: PolicyParseFailure,
        location: Option<(usize, usize)>,
    ) -> Self {
        let details = location
            .filter(|(line, column)| *line > 0 && *column > 0)
            .map(|(line, column)| serde_json::json!({"line": line, "column": column}));
        Self {
            code: "E_POLICY_PARSE".to_string(),
            message: class.summary().to_string(),
            details,
        }
    }

    pub fn result(self) -> anyhow::Result<Value> {
        Ok(serde_json::to_value(serde_json::json!({
             "allowed": false,
             "error": self
        }))?)
    }
}

// ── Shared mapping-stage for direct-tool consumers ────────────────────────
//
// The mapping stage decodes bytes to serde_yaml::Mapping, requires a mapping
// root, and classifies syntax/root errors with preserved serde_yaml location.
// Direct-tool consumers (check_coverage, explain_trace, policy_decide) call
// this instead of constructing their own YAML deserializers.

/// Result of the mapping stage: a validated YAML mapping.
///
/// Carries `serde_yaml::Mapping` — not `serde_yaml::Value` — so downstream
/// code cannot accidentally re-check the root kind, and duplicate-root-key
/// decisions made by serde_yaml's `Mapping` are preserved without a second
/// parse.
pub(crate) struct MappingStage(pub(crate) serde_yaml::Mapping);

/// Decode bytes to a YAML mapping. Returns a ToolError with the approved
/// fixed summary on syntax or root-not-mapping failure, preserving serde_yaml's
/// location (line/column) for syntax errors.
pub(crate) fn yaml_mapping_stage(bytes: &[u8]) -> Result<MappingStage, ToolError> {
    let value: serde_yaml::Value = serde_yaml::from_slice(bytes).map_err(|e| {
        let loc = e.location().map(|l| (l.line(), l.column()));
        ToolError::policy_parse(PolicyParseFailure::YamlSyntax, loc)
    })?;
    match value {
        serde_yaml::Value::Mapping(m) => Ok(MappingStage(m)),
        _ => Err(ToolError::policy_parse(
            PolicyParseFailure::RootNotMapping,
            None,
        )),
    }
}

/// Generic tool helper: decode bytes to a YAML mapping, then deserialize the
/// mapping into a typed policy `T` via `serde_yaml::from_value`. Classifies
/// typed deserialization failure as `PolicyParseFailure::Structure`.
pub(crate) fn parse_tool_policy<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
) -> Result<T, ToolError> {
    let MappingStage(mapping) = yaml_mapping_stage(bytes)?;
    serde_yaml::from_value::<T>(serde_yaml::Value::Mapping(mapping))
        .map_err(|_| ToolError::policy_parse(PolicyParseFailure::Structure, None))
}

pub mod check_args;
pub mod check_coverage;
pub mod check_sequence;
pub mod explain_trace;
pub mod policy_decide;
mod policy_read;

#[cfg(feature = "test-outbound")]
pub mod test_outbound;

pub fn list_tools() -> Vec<Value> {
    #[allow(unused_mut)] // mut needed when feature "test-outbound" is enabled
    let mut list: Vec<Value> = vec![
        serde_json::json!({
            "name": "assay_check_args",
            "description": "Pre-flight review for one proposed MCP tool call. Use this before executing a tool when you have the exact tool name, the JSON arguments that would be sent, and an Assay policy file. It evaluates allow/deny rules plus the matching per-tool JSON schema and returns allowed=true/false, warnings, violations, and a suggested-fix slot. It never invokes the target tool and never proves the provider executed anything.",
            "inputSchema": {
                "type": "object",
                "title": "Tool argument policy check request",
                "description": "Request body for validating one MCP tool call against a local Assay policy.",
                "properties": {
                    "tool": {
                        "type": "string",
                        "description": "Exact MCP tool name to evaluate, using the same name that would appear in the client tool call.",
                        "minLength": 1,
                        "examples": ["github.add_deploy_key", "filesystem.write_file"]
                    },
                    "arguments": {
                        "type": "object",
                        "description": "JSON object that would be sent to the target tool. Assay validates this object against the policy schema for the named tool.",
                        "examples": [
                            { "repository": "owner/repo", "key": "ssh-ed25519 AAAA..." },
                            { "path": "docs/report.md", "content": "draft" }
                        ]
                    },
                    "policy": {
                        "type": "string",
                        "description": "Assay policy file path relative to the server policy root.",
                        "minLength": 1,
                        "examples": ["policy.yaml", "policies/mcp-production.yaml"]
                    }
                },
                "required": ["tool", "arguments", "policy"],
                "examples": [
                    {
                        "tool": "github.add_deploy_key",
                        "arguments": { "repository": "owner/repo", "key": "ssh-ed25519 AAAA..." },
                        "policy": "policy.yaml"
                    }
                ],
                "additionalProperties": false
            }
        }),
        serde_json::json!({
            "name": "assay_check_sequence",
            "description": "Pre-flight review for tool-call order. Use this when an agent is about to make another MCP tool call and you need to check the proposed next tool against policy sequence rules such as required predecessors, forbidden orderings, or deadline windows. It returns allowed=true/false and sequence violations for the trace-so-far plus next_tool. It does not execute tools and does not assert that the workflow is complete.",
            "inputSchema": {
                "type": "object",
                "title": "Tool sequence policy check request",
                "description": "Request body for validating whether next_tool is allowed after the observed history.",
                "properties": {
                    "history": {
                        "type": "array",
                        "description": "Tool names already observed in chronological order before the proposed next call.",
                        "items": { "type": "string", "minLength": 1 },
                        "examples": [["github.get_repository", "github.list_branches"]]
                    },
                    "next_tool": {
                        "type": "string",
                        "description": "Exact MCP tool name being considered as the next call.",
                        "minLength": 1,
                        "examples": ["github.create_pull_request"]
                    },
                    "policy": {
                        "type": "string",
                        "description": "Assay sequence or full policy file path relative to the server policy root.",
                        "minLength": 1,
                        "examples": ["policy.yaml", "policies/release-flow.yaml"]
                    }
                },
                "required": ["history", "next_tool", "policy"],
                "examples": [
                    {
                        "history": ["github.get_repository", "github.list_branches"],
                        "next_tool": "github.create_pull_request",
                        "policy": "policy.yaml"
                    }
                ],
                "additionalProperties": false
            }
        }),
        serde_json::json!({
            "name": "assay_policy_decide",
            "description": "Fast name-only policy decision for an MCP tool. Use this for inexpensive routing or UI gating when only the tool name is known. It checks the policy blocklist and returns allowed=true/false plus a short reason or match. It intentionally does not validate arguments, schemas, sequence rules, runtime delivery, or provider behavior; use assay_check_args for argument-aware review.",
            "inputSchema": {
                "type": "object",
                "title": "Tool name policy decision request",
                "description": "Request body for checking a tool name against the local policy blocklist.",
                "properties": {
                    "tool": {
                        "type": "string",
                        "description": "Exact MCP tool name to check against the policy blocklist.",
                        "minLength": 1,
                        "examples": ["shell.exec", "github.delete_repository"]
                    },
                    "policy": {
                        "type": "string",
                        "description": "Policy file path relative to the server policy root.",
                        "minLength": 1,
                        "examples": ["policy.yaml"]
                    }
                },
                "required": ["tool", "policy"],
                "examples": [
                    {
                        "tool": "shell.exec",
                        "policy": "policy.yaml"
                    }
                ],
                "additionalProperties": false
            }
        }),
        serde_json::json!({
            "name": "assay_check_coverage",
            "description": "Coverage report for policy test traces. Use this after collecting one or more tool-call traces to see which policy tools or rules were exercised, what remains unseen, and whether the requested coverage threshold was met. It returns JSON by default or a Markdown/GitHub annotation summary. This is evidence about trace coverage only, not runtime safety, provider truth, or compliance.",
            "inputSchema": {
                "type": "object",
                "title": "Policy trace coverage request",
                "description": "Request body for comparing recorded tool-call traces with policy coverage expectations.",
                "properties": {
                    "policy": {
                        "type": "string",
                        "description": "Full Assay policy file path relative to the server policy root.",
                        "minLength": 1,
                        "examples": ["policy.yaml", "policies/mcp-production.yaml"]
                    },
                    "traces": {
                        "type": "array",
                        "description": "Recorded traces to compare against the policy. Each trace should list the tools observed in execution order and may list policy rules observed as triggered.",
                        "minItems": 1,
                        "items": {
                            "type": "object",
                            "title": "Coverage trace record",
                            "properties": {
                                "id": {
                                    "type": "string",
                                    "description": "Optional stable trace identifier used in reports.",
                                    "examples": ["trace-pr-42-happy-path"]
                                },
                                "tools": {
                                    "type": "array",
                                    "description": "Tool names observed in this trace.",
                                    "minItems": 1,
                                    "items": { "type": "string", "minLength": 1 },
                                    "examples": [["github.get_repository", "github.create_pull_request"]]
                                },
                                "rules_triggered": {
                                    "type": "array",
                                    "description": "Optional policy rule identifiers observed as triggered in this trace.",
                                    "items": { "type": "string", "minLength": 1 },
                                    "examples": [["require_review_before_merge"]]
                                }
                            },
                            "required": ["tools"],
                            "additionalProperties": false
                        }
                    },
                    "threshold": {
                        "type": "number",
                        "description": "Minimum acceptable coverage percentage from 0 to 100. Defaults to 80.",
                        "minimum": 0,
                        "maximum": 100,
                        "default": 80,
                        "examples": [80, 95]
                    },
                    "format": {
                        "type": "string",
                        "description": "Response format for the coverage report.",
                        "enum": ["json", "markdown", "github"],
                        "default": "json",
                        "examples": ["json", "markdown"]
                    }
                },
                "required": ["policy", "traces"],
                "examples": [
                    {
                        "policy": "policy.yaml",
                        "traces": [
                            {
                                "id": "trace-pr-42-happy-path",
                                "tools": ["github.get_repository", "github.create_pull_request"],
                                "rules_triggered": ["require_review_before_merge"]
                            }
                        ],
                        "threshold": 80,
                        "format": "json"
                    }
                ],
                "additionalProperties": false
            }
        }),
        serde_json::json!({
            "name": "assay_explain_trace",
            "description": "Human-readable explanation of a recorded MCP tool-call trace. Use this when you need to debug or review why a sequence of tool calls was allowed, warned, or blocked by an Assay policy. It evaluates the supplied ordered trace and returns step-by-step rule reasoning, blocked-step counts, and formatted output. It is an offline explanation of supplied evidence, not a live telemetry exporter.",
            "inputSchema": {
                "type": "object",
                "title": "Trace explanation request",
                "description": "Request body for explaining how a recorded tool-call trace evaluates against an Assay policy.",
                "properties": {
                    "policy": {
                        "type": "string",
                        "description": "Assay policy file path relative to the server policy root.",
                        "minLength": 1,
                        "examples": ["policy.yaml"]
                    },
                    "trace": {
                        "type": "array",
                        "description": "Ordered tool-call trace to explain. Each item is one observed MCP tool call.",
                        "minItems": 1,
                        "items": {
                            "type": "object",
                            "title": "Trace step",
                            "properties": {
                                "tool": {
                                    "type": "string",
                                    "description": "Tool name for this trace step.",
                                    "minLength": 1,
                                    "examples": ["github.create_pull_request"]
                                },
                                "args": {
                                    "type": "object",
                                    "description": "Optional JSON arguments observed for this trace step. This is used only for explanation and policy evaluation; the tool is not invoked.",
                                    "examples": [{ "repository": "owner/repo", "title": "Update policy docs" }]
                                }
                            },
                            "required": ["tool"],
                            "additionalProperties": false
                        }
                    },
                    "format": {
                        "type": "string",
                        "description": "Response format for the explanation.",
                        "enum": ["json", "markdown", "terminal", "html"],
                        "default": "json",
                        "examples": ["markdown", "json"]
                    }
                },
                "required": ["policy", "trace"],
                "examples": [
                    {
                        "policy": "policy.yaml",
                        "trace": [
                            {
                                "tool": "github.create_pull_request",
                                "args": { "repository": "owner/repo", "title": "Update policy docs" }
                            }
                        ],
                        "format": "markdown"
                    }
                ],
                "additionalProperties": false
            }
        }),
    ];
    #[cfg(feature = "test-outbound")]
    list.push(serde_json::json!({
        "name": "assay_test_outbound",
        "description": "Test-only: E6a.3 no-pass-through E2E. GET ASSAY_TEST_OUTBOUND_URL with allowlist headers only.",
        "inputSchema": { "type": "object", "properties": {}, "required": [] }
    }));
    list
}

pub async fn handle_call(ctx: &ToolContext, name: &str, args: &Value) -> anyhow::Result<Value> {
    // Membership is owned by `is_known_tool`; callers that skip
    // `classify_call_tool_params` still must not reflect `name`.
    if !is_known_tool(name) {
        return Err(anyhow::anyhow!("unknown tool"));
    }
    match name {
        "assay_check_args" => check_args::check_args(ctx, args).await,
        "assay_check_sequence" => check_sequence::check_sequence(ctx, args).await,
        "assay_policy_decide" => policy_decide::policy_decide(ctx, args).await,
        "assay_check_coverage" => check_coverage::check_coverage(ctx, args).await,
        "assay_explain_trace" => explain_trace::explain_trace(ctx, args).await,
        #[cfg(feature = "test-outbound")]
        "assay_test_outbound" => test_outbound::test_outbound(args).await,
        _ => unreachable!("is_known_tool accepted an unhandled name"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact UTF-8-safe prefix: 4094 ASCII `X` bytes.
    ///
    /// The boundary-bisecting message is 4094 `X` + 🔒 (4 bytes, F0 9F 94 92) + ASCII tail.
    /// Byte 4096 falls inside the emoji. The UTF-8-safe prefix at 4096 bytes is the last
    /// char boundary at-or-before 4096, which is byte 4094, so the prefix is `"X".repeat(4094)`.
    fn expected_bounded_prefix() -> String {
        "X".repeat(4094)
    }

    /// Build a message whose byte 4096 falls inside a 4-byte code point.
    fn boundary_bisecting_message() -> String {
        let prefix = "X".repeat(4094);
        format!("{prefix}🔒tail_that_must_not_appear")
    }

    #[test]
    fn constructor_bounds_message_to_exact_prefix() {
        let message = boundary_bisecting_message();
        let error = ToolError::new("E_TEST", &message);
        let expected = expected_bounded_prefix();

        assert_eq!(
            error.message, expected,
            "constructor must produce the exact UTF-8-safe prefix"
        );
        assert!(
            error.message.len() <= 4096,
            "constructor result exceeds 4096 bytes: {}",
            error.message.len()
        );
    }

    #[test]
    fn serializer_bounds_direct_struct_to_exact_prefix() {
        let message = boundary_bisecting_message();
        let expected = expected_bounded_prefix();
        let error = ToolError {
            code: "E_TEST".into(),
            message: message.clone(),
            details: None,
        };
        let serialized = serde_json::to_value(&error).unwrap();
        let msg = serialized["message"].as_str().unwrap();
        assert_eq!(
            msg, expected,
            "serializer must produce the exact UTF-8-safe prefix for direct struct"
        );
    }

    #[test]
    fn serializer_bounds_post_mutation_to_exact_prefix() {
        let message = boundary_bisecting_message();
        let expected = expected_bounded_prefix();
        let mut error = ToolError::new("E_TEST", "short");
        error.message = message;
        let serialized = serde_json::to_value(&error).unwrap();
        let msg = serialized["message"].as_str().unwrap();
        assert_eq!(
            msg, expected,
            "serializer must produce the exact UTF-8-safe prefix for post-mutation"
        );
    }

    #[test]
    fn result_publication_bounds_direct_struct() {
        let message = boundary_bisecting_message();
        let expected = expected_bounded_prefix();
        let error = ToolError {
            code: "E_TEST".into(),
            message: message.clone(),
            details: None,
        };
        let result = error.result().unwrap();
        let msg = result
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap();
        assert_eq!(
            msg, expected,
            "result() must produce the exact UTF-8-safe prefix for direct struct"
        );
    }

    #[test]
    fn result_publication_bounds_post_mutation() {
        let message = boundary_bisecting_message();
        let expected = expected_bounded_prefix();
        let mut error = ToolError::new("E_TEST", "short");
        error.message = message;
        let result = error.result().unwrap();
        let msg = result
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap();
        assert_eq!(
            msg, expected,
            "result() must produce the exact UTF-8-safe prefix for post-mutation"
        );
    }

    #[test]
    fn all_three_paths_produce_same_bounded_prefix() {
        let message = boundary_bisecting_message();
        let expected = expected_bounded_prefix();

        let via_constructor = ToolError::new("E_TEST", &message);
        assert_eq!(via_constructor.message, expected);

        let direct = ToolError {
            code: "E_TEST".into(),
            message: message.clone(),
            details: None,
        };
        let direct_serialized = serde_json::to_value(&direct).unwrap();
        let direct_msg = direct_serialized["message"].as_str().unwrap();
        assert_eq!(direct_msg, expected);

        let mut mutated = ToolError::new("E_TEST", "short");
        mutated.message = message;
        let mutated_serialized = serde_json::to_value(&mutated).unwrap();
        let mutated_msg = mutated_serialized["message"].as_str().unwrap();
        assert_eq!(mutated_msg, expected);
    }

    #[test]
    fn ascii_message_ceiling_is_exactly_4096_bytes() {
        let exact = "A".repeat(4096);
        let over = "B".repeat(4097);

        let exact_error = ToolError::new("E_TEST", &exact);
        assert_eq!(exact_error.message, exact);

        let over_error = ToolError::new("E_TEST", &over);
        assert_eq!(
            over_error.message.len(),
            4096,
            "constructor must enforce the literal public 4096-byte contract"
        );
        assert_eq!(over_error.message, "B".repeat(4096));

        let direct = ToolError {
            code: "E_TEST".into(),
            message: over,
            details: None,
        };
        let serialized = serde_json::to_value(direct).unwrap();
        let published = serialized["message"].as_str().unwrap();
        assert_eq!(
            published.len(),
            4096,
            "serializer must enforce the literal public 4096-byte contract"
        );
        assert_eq!(published, "B".repeat(4096));
    }

    #[test]
    fn policy_parse_publishes_only_positive_locations() {
        let positive = ToolError::policy_parse(PolicyParseFailure::YamlSyntax, Some((2, 3)));
        assert_eq!(
            positive.details,
            Some(serde_json::json!({"line": 2, "column": 3}))
        );

        for location in [(0, 3), (2, 0), (0, 0)] {
            let error = ToolError::policy_parse(PolicyParseFailure::YamlSyntax, Some(location));
            assert_eq!(
                error.details, None,
                "non-positive location {location:?} must not be published"
            );
        }
    }
}
