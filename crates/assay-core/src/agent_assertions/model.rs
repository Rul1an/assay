assay_common::closed! {
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
    pub enum TraceAssertion tagged "type" {
        "trace_must_call_tool" => TraceMustCallTool {
            tool: String,
            min_calls: Option<u32>,
        },
        "trace_must_not_call_tool" => TraceMustNotCallTool { tool: String },
        "trace_tool_sequence" => TraceToolSequence {
            sequence: Vec<String>,
            allow_other_tools: bool,
        },
        "trace_max_steps" => TraceMaxSteps { max: u32 },
        "args_valid" => ArgsValid {
            tool: String,
            #[opaque_json = "test_args"]
            test_args: Option<serde_json::Value>,
            #[opaque_json = "policy"]
            policy: Option<serde_json::Value>,
            #[default]
            expect: Option<String>,
        },
        "sequence_valid" => SequenceValid {
            #[default]
            test_trace: Option<Vec<crate::storage::rows::ToolCallRow>>,
            #[opaque_json_rows = "test_trace_raw"]
            test_trace_raw: Option<Vec<serde_json::Value>>,
            #[opaque_json = "policy"]
            policy: Option<serde_json::Value>,
            #[default]
            expect: Option<String>,
        },
        "tool_blocklist" => ToolBlocklist {
            #[default]
            test_tool_calls: Option<Vec<String>>,
            #[opaque_json = "policy"]
            policy: Option<serde_json::Value>,
            #[default]
            expect: Option<String>,
        },
    }
}
