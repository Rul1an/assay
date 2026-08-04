use serde::{Deserialize, Serialize};

/// An assertion carrying a key this enum does not define is rejected at parse time.
///
/// Without `deny_unknown_fields` serde drops the unrecognised key silently, and where the
/// intended field has a default the assertion falls back to a shape that cannot fail. The
/// documented `max_calls: 0` "must NOT use a forbidden tool" example is the worked case: the
/// key is dropped, `min_calls` defaults to 1 in `matchers.rs`, and the assertion inverts into
/// "must be called at least once" with no signal at any stage (#1961).
///
/// `deny_unknown_fields` is applied at the **container**, which is the only place serde accepts
/// it — it is not a variant attribute, and the compiler rejects it as one. There is folklore
/// that container-level rejection is unreliable on an internally-tagged enum (serde-rs/serde
/// #2294, #1358). Those defects are about unit-like variants and `flatten`; every variant here
/// is a struct variant with named fields and none flattens, and rejection was verified on this
/// exact shape for the hardest case in it — `tool_blocklist`, whose fields are all defaulted, so
/// nothing but the tag is required. A stray key there is rejected too. Nested free-form values
/// (`policy`, `test_args`) are `serde_json::Value` and stay unconstrained, which is intended:
/// the guard covers the assertion's own field vocabulary, not policy contents.
///
/// Keep the guard at the container. Per-variant allow-set validation was considered and is not
/// needed here; the tests in `tests/assertions_unknown_fields.rs` pin the behaviour that makes
/// it unnecessary, including the all-defaulted variant.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TraceAssertion {
    #[serde(rename = "trace_must_call_tool")]
    TraceMustCallTool {
        tool: String,
        min_calls: Option<u32>,
    },
    #[serde(rename = "trace_must_not_call_tool")]
    TraceMustNotCallTool { tool: String },
    #[serde(rename = "trace_tool_sequence")]
    TraceToolSequence {
        sequence: Vec<String>,
        allow_other_tools: bool,
    },
    #[serde(rename = "trace_max_steps")]
    TraceMaxSteps { max: u32 },
    #[serde(rename = "args_valid")]
    ArgsValid {
        tool: String,
        #[serde(default)]
        test_args: Option<serde_json::Value>,
        #[serde(default)]
        policy: Option<serde_json::Value>,
        #[serde(default)]
        expect: Option<String>,
    },
    #[serde(rename = "sequence_valid")]
    SequenceValid {
        #[serde(default)]
        test_trace: Option<Vec<crate::storage::rows::ToolCallRow>>, // Reusing existing struct or simplified Value
        // If the user uses simplified structure in yaml, we might need a custom struct or Value.
        // fp_suite uses: - tool: VerifyIdentity, args: {}
        // ToolCallRow is a bit heavy, let's use Value for flexibility if model mismatch.
        // But for safety, let's look at strict parsing.
        // Example: { tool: "VerifyIdentity", args: {} }
        #[serde(default)]
        test_trace_raw: Option<Vec<serde_json::Value>>,
        #[serde(default)]
        policy: Option<serde_json::Value>,
        #[serde(default)]
        expect: Option<String>,
    },
    #[serde(rename = "tool_blocklist")]
    ToolBlocklist {
        #[serde(default)]
        test_tool_calls: Option<Vec<String>>,
        #[serde(default)]
        policy: Option<serde_json::Value>,
        #[serde(default)]
        expect: Option<String>,
    },
}
