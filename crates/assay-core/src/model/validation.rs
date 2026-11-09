use crate::on_error::ErrorPolicy;

use super::types::{
    EvalConfig, Expected, SequenceRule, Settings, TestCase, TestStatus, ThresholdingConfig,
};

/// Tolerance used by the semantic-similarity evaluator at its pass boundary.
pub const SEMANTIC_SIMILARITY_EPSILON: f64 = 1e-6;

pub(crate) fn is_default_otel(o: &crate::config::otel::OtelConfig) -> bool {
    o == &crate::config::otel::OtelConfig::default()
}

pub(crate) fn is_default_thresholds(t: &crate::thresholds::ThresholdConfig) -> bool {
    t == &crate::thresholds::ThresholdConfig::default()
}

pub(crate) fn is_default_error_policy(p: &ErrorPolicy) -> bool {
    *p == ErrorPolicy::default()
}

pub(crate) fn is_default_settings(s: &Settings) -> bool {
    s == &Settings::default()
}

/// The field or field group that leaves `expected` without an effective check.
///
/// Parsing and execution share this predicate because both must reject an
/// explicitly inert assertion. Serialization deliberately uses a narrower
/// predicate: only the synthetic omitted-key sentinel may disappear on write.
pub(crate) fn vacuous_expected_field(e: &Expected) -> Option<&'static str> {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "a variant with no collection to be empty cannot be vacuous in this sense; a new one that can must be named above or its vacuous form goes unreported (#1949 layer 1)"
    )]
    match e {
        Expected::MustContain { must_contain } if must_contain.iter().all(String::is_empty) => {
            Some("must_contain")
        }
        Expected::MustNotContain { must_not_contain } if must_not_contain.is_empty() => {
            Some("must_not_contain")
        }
        Expected::RegexMatch { pattern, .. } if pattern.is_empty() => Some("pattern"),
        Expected::SemanticSimilarityTo { min_score, .. }
            if *min_score <= -1.0 + SEMANTIC_SIMILARITY_EPSILON =>
        {
            Some("min_score")
        }
        Expected::ArgsValid { policy, schema }
            if schema.as_ref().is_some_and(args_policy_asserts_nothing)
                || (policy.is_none() && schema.is_none()) =>
        {
            Some("policy/schema")
        }
        Expected::SequenceValid {
            policy,
            sequence,
            rules,
        } if (sequence.is_none() && rules.is_none() && policy.is_none())
            || (sequence.is_none() && rules.as_ref().is_some_and(Vec::is_empty)) =>
        {
            Some("policy/sequence/rules")
        }
        Expected::ToolOutputValid { schemas }
            if schemas.as_ref().is_none_or(schema_map_asserts_nothing) =>
        {
            Some("schemas")
        }
        Expected::ToolBlocklist { blocked } if blocked.is_empty() => Some("blocked"),
        _ => None,
    }
}

/// Decide whether an `args_valid` policy asserts anything.
///
/// For a structured policy this mirrors the control-surface half of
/// `validate_args_policy_value` exactly — merged root and `tools` allow/deny
/// lists, `any` for the universality test, and `deny` as the only enforcement
/// mode that makes a policy effective. Two approximations of the same rule
/// drift; `args_policy_oracles_agree` pins them together.
fn args_policy_asserts_nothing(schema: &serde_json::Value) -> bool {
    let Some(root) = schema.as_object() else {
        return schema_map_asserts_nothing(schema);
    };
    // The root is itself a JSON Schema rather than a tool map.
    if root
        .keys()
        .any(|key| key != "$defs" && is_json_schema_keyword(key))
    {
        return false;
    }
    if has_structured_args_policy_shape(schema) {
        let schemas_assert = root
            .get("schemas")
            .is_some_and(|schemas| !schema_map_asserts_nothing(schemas));
        if schemas_assert || structured_controls_assert(root) {
            return false;
        }
        // Claim vacuity only for a policy this rule fully understands. Anything
        // else — an unenforced field, a malformed list, a non-mapping `schemas`
        // — has a specific downstream diagnosis that is more useful than a
        // generic "asserts nothing", so defer to it.
        return structured_policy_is_fully_understood(root);
    }
    schema_map_asserts_nothing(schema)
}

/// Mirror of `validate_args_policy_value`'s control-surface effectiveness rule.
/// Whether every part of a structured policy is something this rule models.
///
/// The vacuity verdict is only honest for a policy it fully understands. For
/// anything else the execution-time check has a specific message ("not enforced
/// by this evaluator", "must be a list", "must be a mapping") that is far more
/// useful than "asserts nothing", so those defer to it.
fn structured_policy_is_fully_understood(
    root: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    if !root
        .keys()
        .all(|key| ARGS_POLICY_READ_FIELDS.contains(&key.as_str()))
    {
        return false;
    }
    fn well_formed_list(value: Option<&serde_json::Value>) -> bool {
        value.is_none_or(|value| {
            value
                .as_array()
                .is_some_and(|entries| entries.iter().all(serde_json::Value::is_string))
        })
    }
    if !well_formed_list(root.get("allow")) || !well_formed_list(root.get("deny")) {
        return false;
    }
    if let Some(tools) = root.get("tools") {
        let Some(tools) = tools.as_object() else {
            return false;
        };
        // Execution enforces only `allow`/`deny`; every other key gets its own
        // "not enforced by this evaluator" diagnosis.
        if !tools.keys().all(|k| matches!(k.as_str(), "allow" | "deny")) {
            return false;
        }
        if !well_formed_list(tools.get("allow")) || !well_formed_list(tools.get("deny")) {
            return false;
        }
    }
    if root.get("schemas").is_some_and(|s| !s.is_object()) {
        return false;
    }
    root.get("enforcement")
        .is_none_or(serde_json::Value::is_object)
}

fn structured_controls_assert(root: &serde_json::Map<String, serde_json::Value>) -> bool {
    fn list(value: Option<&serde_json::Value>) -> Vec<&str> {
        value
            .and_then(serde_json::Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .collect()
            })
            .unwrap_or_default()
    }
    let mut allow = list(root.get("allow"));
    let mut deny = list(root.get("deny"));
    if let Some(tools) = root.get("tools").and_then(serde_json::Value::as_object) {
        allow.extend(list(tools.get("allow")));
        deny.extend(list(tools.get("deny")));
    }
    if !deny.is_empty()
        || (!allow.is_empty()
            && !allow
                .iter()
                .any(|pattern| is_universal_tool_pattern(pattern)))
    {
        return true;
    }
    root.get("enforcement")
        .and_then(serde_json::Value::as_object)
        .and_then(|enforcement| enforcement.get("unconstrained_tools"))
        .and_then(serde_json::Value::as_str)
        == Some("deny")
}

fn schema_map_asserts_nothing(value: &serde_json::Value) -> bool {
    if !value.is_object() {
        return false;
    }
    // Delegate the $defs merge to the one preparation implementation instead of hand-rolling a
    // copy of it here; two approximations of the same merge drift (#1951). A map that fails to
    // prepare (a $defs collision, a non-mapping $defs) is NOT classified as vacuous: preparation
    // failure is a malformed config with its own loud diagnosis at the validation and runtime
    // layers, and claiming vacuity (or effectiveness) for it here would mask that.
    let Ok(prepared) = crate::policy_engine::prepare_schema_map(value) else {
        return false;
    };
    prepared.as_object().is_some_and(|schemas| {
        schemas
            .iter()
            .all(|(_, schema)| schema_asserts_nothing(schema))
    })
}

fn schema_asserts_nothing(schema: &serde_json::Value) -> bool {
    let dialect = SchemaDialect::from_schema(schema);
    schema_asserts_nothing_inner(schema, schema, dialect, 0)
}

#[derive(Clone, Copy)]
enum SchemaDialect {
    Draft4,
    Draft6,
    Draft7,
    Modern,
}

impl SchemaDialect {
    fn from_schema(schema: &serde_json::Value) -> Self {
        match schema
            .get("$schema")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
        {
            value if value.contains("draft-04") => Self::Draft4,
            value if value.contains("draft-06") => Self::Draft6,
            value if value.contains("draft-07") => Self::Draft7,
            _ => Self::Modern,
        }
    }

    fn is_legacy(self) -> bool {
        matches!(self, Self::Draft4 | Self::Draft6 | Self::Draft7)
    }
}

fn schema_asserts_nothing_inner(
    schema: &serde_json::Value,
    root: &serde_json::Value,
    dialect: SchemaDialect,
    depth: usize,
) -> bool {
    if depth > 64 {
        return false;
    }
    match schema {
        serde_json::Value::Bool(value) => *value,
        serde_json::Value::Object(schema) => {
            let direct_assertion = schema.iter().any(|(keyword, value)| {
                schema_keyword_asserts(keyword, value, schema, root, dialect, depth + 1)
            });
            let conditional_assertion = conditional_asserts(schema, root, dialect, depth + 1);
            !direct_assertion && !conditional_assertion
        }
        _ => false,
    }
}

fn schema_keyword_asserts(
    keyword: &str,
    value: &serde_json::Value,
    containing_schema: &serde_json::Map<String, serde_json::Value>,
    root: &serde_json::Value,
    dialect: SchemaDialect,
    depth: usize,
) -> bool {
    match keyword {
        "$ref" => local_ref_target(value, root)
            .is_none_or(|target| !schema_asserts_nothing_inner(target, root, dialect, depth)),
        "$dynamicRef" | "$recursiveRef" | "type" | "enum" | "multipleOf" | "maximum"
        | "minimum" | "maxLength" | "maxItems" | "maxProperties" => true,
        "const" => !matches!(dialect, SchemaDialect::Draft4),
        "exclusiveMaximum" | "exclusiveMinimum" => value.as_bool() != Some(false),
        "pattern" => value.as_str() != Some(""),
        "minLength" | "minItems" | "minProperties" => {
            value.as_u64().is_none_or(|minimum| minimum > 0)
        }
        "uniqueItems" => value.as_bool() != Some(false),
        "required" => value.as_array().is_some_and(|entries| !entries.is_empty()),
        "dependentRequired" if !dialect.is_legacy() => value.as_object().is_some_and(|entries| {
            entries
                .values()
                .any(|required| required.as_array().is_some_and(|names| !names.is_empty()))
        }),
        "properties" | "patternProperties" => value.as_object().is_some_and(|schemas| {
            schemas
                .values()
                .any(|schema| !schema_asserts_nothing_inner(schema, root, dialect, depth))
        }),
        "dependentSchemas" if !dialect.is_legacy() => value.as_object().is_some_and(|schemas| {
            schemas
                .values()
                .any(|schema| !schema_asserts_nothing_inner(schema, root, dialect, depth))
        }),
        // jsonschema compiles `dependencies` for every draft that declares the
        // applicator vocabulary (keywords/mod.rs: `(_, "dependencies")`), so it
        // asserts under the modern dialect too even though the spec dropped it.
        "dependencies" => value.as_object().is_some_and(|dependencies| {
            dependencies.values().any(|dependency| {
                dependency
                    .as_array()
                    .is_some_and(|required| !required.is_empty())
                    || ((dependency.is_object() || dependency.is_boolean())
                        && !schema_asserts_nothing_inner(dependency, root, dialect, depth))
            })
        }),
        "additionalProperties" | "items" => {
            !schema_asserts_nothing_inner(value, root, dialect, depth)
        }
        "unevaluatedProperties" | "unevaluatedItems" if !dialect.is_legacy() => {
            !schema_asserts_nothing_inner(value, root, dialect, depth)
        }
        "propertyNames" if !matches!(dialect, SchemaDialect::Draft4) => {
            !schema_asserts_nothing_inner(value, root, dialect, depth)
        }
        "additionalItems"
            if dialect.is_legacy()
                && containing_schema
                    .get("items")
                    .is_some_and(serde_json::Value::is_array) =>
        {
            !schema_asserts_nothing_inner(value, root, dialect, depth)
        }
        "prefixItems" if !dialect.is_legacy() => value.as_array().is_some_and(|schemas| {
            schemas
                .iter()
                .any(|schema| !schema_asserts_nothing_inner(schema, root, dialect, depth))
        }),
        "allOf" => value.as_array().is_some_and(|schemas| {
            schemas
                .iter()
                .any(|schema| !schema_asserts_nothing_inner(schema, root, dialect, depth))
        }),
        "anyOf" => value.as_array().is_some_and(|schemas| {
            schemas.is_empty()
                || schemas
                    .iter()
                    .all(|schema| !schema_asserts_nothing_inner(schema, root, dialect, depth))
        }),
        "oneOf" => value
            .as_array()
            .is_some_and(|schemas| match schemas.as_slice() {
                [] => true,
                [schema] => !schema_asserts_nothing_inner(schema, root, dialect, depth),
                schemas
                    if schemas
                        .iter()
                        .filter(|schema| schema.as_bool() == Some(true))
                        .count()
                        == 1
                        && schemas.iter().all(serde_json::Value::is_boolean) =>
                {
                    false
                }
                _ => true,
            }),
        "not" => value != &serde_json::Value::Bool(false),
        "contains" if matches!(dialect, SchemaDialect::Draft4) => false,
        "contains" if dialect.is_legacy() => true,
        "contains" => {
            containing_schema
                .get("minContains")
                .and_then(serde_json::Value::as_u64)
                != Some(0)
                || containing_schema.contains_key("maxContains")
        }
        "format" if dialect.is_legacy() => is_known_format(value),
        _ => false,
    }
}

fn conditional_asserts(
    schema: &serde_json::Map<String, serde_json::Value>,
    root: &serde_json::Value,
    dialect: SchemaDialect,
    depth: usize,
) -> bool {
    if matches!(dialect, SchemaDialect::Draft4 | SchemaDialect::Draft6) {
        return false;
    }
    let Some(condition) = schema.get("if") else {
        return false;
    };
    let branch_asserts = |keyword| {
        schema
            .get(keyword)
            .is_some_and(|branch| !schema_asserts_nothing_inner(branch, root, dialect, depth))
    };
    match condition.as_bool() {
        Some(true) => branch_asserts("then"),
        Some(false) => branch_asserts("else"),
        None => branch_asserts("then") || branch_asserts("else"),
    }
}

fn local_ref_target<'a>(
    reference: &serde_json::Value,
    root: &'a serde_json::Value,
) -> Option<&'a serde_json::Value> {
    let reference = reference.as_str()?;
    let fragment = if let Some(fragment) = reference.strip_prefix('#') {
        fragment
    } else {
        let root_id = root.get("$id")?.as_str()?;
        reference.strip_prefix(root_id)?.strip_prefix('#')?
    };
    if fragment.is_empty() {
        return None;
    }
    root.pointer(fragment)
}

fn is_known_format(value: &serde_json::Value) -> bool {
    matches!(
        value.as_str(),
        Some(
            "date"
                | "date-time"
                | "email"
                | "hostname"
                | "ipv4"
                | "ipv6"
                | "regex"
                | "time"
                | "uri"
                | "uri-reference"
                | "uri-template"
                | "uuid"
        )
    )
}

/// Explain an `Expected` shape that the current metric set cannot execute as written.
pub(crate) fn non_executable_expected_reason(e: &Expected) -> Option<&'static str> {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "only sequence-bearing variants can be non-executable; the rest have no rule set to be unreachable"
    )]
    match e {
        Expected::JudgeCriteria { .. } => Some("judge_criteria has no registered evaluator"),
        // `sequence_valid` hands every rule to `sequence_eval::evaluate_rules`, the one evaluator
        // of this language (#2112), so every kind is executable. #1948 refused five kinds here
        // while the metric still carried its own evaluator and passed them through a `_` arm;
        // the refusal outlived that reason and kept `assay run` from executing rules the metric
        // already handled. No wildcard: a new kind is classified here on purpose, not accepted
        // by default.
        Expected::SequenceValid {
            rules: Some(rules), ..
        } => rules.iter().find_map(|rule| match rule {
            SequenceRule::Require { .. }
            | SequenceRule::Eventually { .. }
            | SequenceRule::MaxCalls { .. }
            | SequenceRule::Before { .. }
            | SequenceRule::After { .. }
            | SequenceRule::NeverAfter { .. }
            | SequenceRule::Sequence { .. }
            | SequenceRule::Blocklist { .. } => None,
        }),
        _ => None,
    }
}

pub(crate) fn ineffective_expected_reason(e: &Expected) -> Option<&'static str> {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "only the variants named above can be ineffective from their config alone; layer 2 covers what the config cannot decide"
    )]
    match e {
        Expected::MustNotContain { must_not_contain }
            if must_not_contain.iter().any(String::is_empty) =>
        {
            Some("must_not_contain contains an empty string, so no response can pass")
        }
        Expected::RegexNotMatch { pattern, .. } if pattern.is_empty() => {
            Some("an empty regex_not_match pattern matches every response, so no response can pass")
        }
        Expected::SequenceValid {
            rules: Some(rules), ..
        } if rules.iter().any(|rule| {
            matches!(
                rule,
                SequenceRule::Before { first, then } if first == then
            )
        }) =>
        {
            Some("a before rule with identical tools cannot constrain a trace")
        }
        _ => None,
    }
}

/// Reject any expected value that cannot safely reach metric dispatch.
pub(crate) fn validate_expected_for_execution(e: &Expected) -> anyhow::Result<()> {
    if matches!(e, Expected::Reference { .. }) {
        anyhow::bail!("unresolved `$ref` cannot be executed; resolve or migrate it first");
    }
    if let Some(field) = vacuous_expected_field(e) {
        anyhow::bail!("`{field}` asserts nothing");
    }
    if let Some(reason) = non_executable_expected_reason(e) {
        anyhow::bail!("expected block is not executable: {reason}");
    }
    if let Some(reason) = ineffective_expected_reason(e) {
        anyhow::bail!("{reason}");
    }
    validate_static_inputs(e)?;
    Ok(())
}

fn validate_static_inputs(e: &Expected) -> anyhow::Result<()> {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "only variants carrying a policy path have static inputs to validate"
    )]
    match e {
        Expected::RegexMatch { pattern, flags } | Expected::RegexNotMatch { pattern, flags } => {
            let mut builder = regex::RegexBuilder::new(pattern);
            for flag in flags {
                match flag.as_str() {
                    "i" => {
                        builder.case_insensitive(true);
                    }
                    "m" => {
                        builder.multi_line(true);
                    }
                    "s" => {
                        builder.dot_matches_new_line(true);
                    }
                    _ => {}
                }
            }
            builder
                .build()
                .map_err(|e| anyhow::anyhow!("invalid regex pattern: {e}"))?;
        }
        Expected::JsonSchema {
            json_schema,
            schema_file,
        } => {
            let source = if let Some(path) = schema_file {
                std::fs::read_to_string(path)
                    .map_err(|e| anyhow::anyhow!("failed to read schema_file '{path}': {e}"))?
            } else {
                json_schema.clone()
            };
            let schema: serde_json::Value = serde_json::from_str(&source)
                .map_err(|e| anyhow::anyhow!("invalid JSON schema: {e}"))?;
            crate::policy_engine::compile_schema(&schema)
                .map_err(|e| anyhow::anyhow!("schema compile failed: {e}"))?;
        }
        Expected::ArgsValid {
            schema: Some(schema),
            ..
        } => validate_args_policy_value(schema)?,
        Expected::ToolOutputValid {
            schemas: Some(schema),
        } => {
            // Prepare BEFORE validating, exactly as the structured args path does above, so a
            // shared-$defs collision or a non-mapping $defs is a loud config error here rather
            // than a silently vacuous schema at evaluation time (#1951). Validation, the vacuity
            // rule, and the runtime metric all reason about the SAME prepared map.
            let prepared = crate::policy_engine::prepare_schema_map(schema)
                .map_err(|e| anyhow::anyhow!("tool_output_valid schemas: {e}"))?;
            validate_schema_map(&prepared, false, true)?
        }
        Expected::ArgsValid {
            policy: Some(path),
            schema: None,
        } => validate_args_policy(path)?,
        Expected::SequenceValid {
            policy: Some(path), ..
        } => validate_sequence_policy(path)?,
        _ => {}
    }
    Ok(())
}

/// Replace file-backed assertion inputs with an immutable execution snapshot.
///
/// The returned `Expected` value is what validation, fingerprinting, and metric
/// evaluation must all consume. This prevents incremental-cache drift and avoids
/// a second file read after provider dispatch.
pub(crate) fn bind_external_expected_inputs(e: &mut Expected) -> anyhow::Result<()> {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "only variants carrying an external reference need binding"
    )]
    match e {
        Expected::JsonSchema {
            json_schema,
            schema_file,
        } => {
            if let Some(path) = schema_file.take() {
                *json_schema = std::fs::read_to_string(&path)
                    .map_err(|err| anyhow::anyhow!("failed to read schema_file '{path}': {err}"))?;
            }
        }
        Expected::ArgsValid { policy, schema } if schema.is_none() => {
            if let Some(path) = policy.take() {
                let source = std::fs::read_to_string(&path).map_err(|err| {
                    anyhow::anyhow!("failed to read args_valid policy '{path}': {err}")
                })?;
                *schema = Some(
                    serde_yaml::from_str(&source)
                        .map_err(|err| anyhow::anyhow!("invalid args_valid policy YAML: {err}"))?,
                );
            }
        }
        Expected::SequenceValid {
            policy,
            sequence,
            rules,
        } => {
            if let Some(path) = policy.take() {
                let source = std::fs::read_to_string(&path).map_err(|err| {
                    anyhow::anyhow!("failed to read sequence_valid policy '{path}': {err}")
                })?;
                match read_sequence_policy_rules(&path, &source)? {
                    SequencePolicyFile::ExactSequence(loaded) => {
                        if sequence.is_none() {
                            *sequence = Some(loaded);
                        }
                    }
                    SequencePolicyFile::Rules(loaded) => {
                        if rules.is_none() {
                            *rules = Some(loaded);
                        }
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_args_policy(path: &str) -> anyhow::Result<()> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("failed to read args_valid policy '{path}': {e}"))?;
    let policy: serde_json::Value = serde_yaml::from_str(&source)
        .map_err(|e| anyhow::anyhow!("invalid args_valid policy YAML: {e}"))?;

    validate_args_policy_value(&policy)
}

/// The root fields of a structured `args_valid` policy: the ones the evaluator reads, and the
/// `version` and `name` labels, as `assay mcp wrap` has them. Execution refuses any other field by
/// name, and the load-time rule claims to understand only a policy made of these.
const ARGS_POLICY_READ_FIELDS: &[&str] = &[
    "version",
    "name",
    "allow",
    "deny",
    "tools",
    "schemas",
    "enforcement",
];

/// Validate an inline `args_valid` policy using the execution-time contract.
pub fn validate_args_policy_value(policy: &serde_json::Value) -> anyhow::Result<()> {
    if policy
        .as_object()
        .and_then(|root| root.get("version"))
        .is_some_and(|version| !version.is_string())
    {
        anyhow::bail!(
            "args_valid policy version must be a string; move a legacy tool named `version` under `version: \"2.0\"` and `schemas.version`"
        );
    }
    if policy
        .as_object()
        .is_some_and(|root| root.len() == 1 && root.contains_key("schemas"))
    {
        anyhow::bail!(
            "args_valid policy with only `schemas` is ambiguous; add `version: \"2.0\"` for a structured policy, including a tool named `schemas`"
        );
    }

    let structured = has_structured_args_policy_shape(policy);

    if structured {
        // Every root field the evaluator does not read is refused by name, and every
        // `enforcement` key but `unconstrained_tools`. `constraints`, `limits`, `tool_pins` and
        // the other fields `assay mcp wrap` reads were refused before; a misspelled or misplaced
        // field (`schema`, `sequences`, a root `$defs`) was ignored, and a deny list kept such a
        // policy effective while the content it dropped checked nothing.
        let mut unsupported: Vec<String> = policy
            .as_object()
            .map(|root| {
                root.keys()
                    .filter(|key| !ARGS_POLICY_READ_FIELDS.contains(&key.as_str()))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if let Some(enforcement) = policy
            .get("enforcement")
            .and_then(serde_json::Value::as_object)
        {
            unsupported.extend(
                enforcement
                    .keys()
                    .filter(|key| key.as_str() != "unconstrained_tools")
                    .map(|key| format!("enforcement.{key}")),
            );
        }
        unsupported.sort_unstable();
        if !unsupported.is_empty() {
            anyhow::bail!(
                "args_valid policy fields are not enforced by this evaluator: {}",
                unsupported.join(", ")
            );
        }

        let mut allow = policy_string_list(policy.get("allow"), "allow")?;
        let mut deny = policy_string_list(policy.get("deny"), "deny")?;
        if let Some(tools) = policy.get("tools") {
            let tools = tools
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("args_valid policy tools must be a mapping"))?;
            let mut unsupported: Vec<_> = tools
                .keys()
                .filter(|key| !matches!(key.as_str(), "allow" | "deny"))
                .map(|key| format!("tools.{key}"))
                .collect();
            unsupported.sort_unstable();
            if !unsupported.is_empty() {
                let migration = if tools.contains_key("arg_constraints") {
                    ". Put each tool's JSON Schema under `schemas.<tool>` in a `version: \"2.0\"` \
                     policy, or under the test's inline `schema:`; a map of argument names to \
                     constraints is not a JSON Schema, so describe the arguments under `properties`"
                } else {
                    ""
                };
                anyhow::bail!(
                    "args_valid policy fields are not enforced by this evaluator: {}{migration}",
                    unsupported.join(", ")
                );
            }
            allow.extend(policy_string_list(tools.get("allow"), "tools.allow")?);
            deny.extend(policy_string_list(tools.get("deny"), "tools.deny")?);
        }
        let mut effective = !deny.is_empty()
            || (!allow.is_empty()
                && !allow
                    .iter()
                    .any(|pattern| is_universal_tool_pattern(pattern)));

        if let Some(schemas) = policy.get("schemas") {
            let schemas = schemas
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("args_valid policy schemas must be a mapping"))?;
            if !schemas.is_empty() {
                let schemas = serde_json::Value::Object(schemas.clone());
                let prepared_schemas = crate::policy_engine::prepare_schema_map(&schemas)
                    .map_err(anyhow::Error::msg)?;
                if !prepared_schemas
                    .as_object()
                    .is_some_and(serde_json::Map::is_empty)
                {
                    validate_schema_map(&prepared_schemas, false, false)?;
                }
                effective |= !schema_map_asserts_nothing(&schemas);
            }
        }

        if let Some(enforcement) = policy.get("enforcement") {
            let enforcement = enforcement.as_object().ok_or_else(|| {
                anyhow::anyhow!("args_valid policy enforcement must be a mapping")
            })?;
            if let Some(mode) = enforcement.get("unconstrained_tools") {
                let mode = mode.as_str().ok_or_else(|| {
                    anyhow::anyhow!(
                        "args_valid policy enforcement.unconstrained_tools must be a string"
                    )
                })?;
                match mode {
                    "deny" => effective = true,
                    "warn" | "allow" => {}
                    _ => anyhow::bail!(
                        "args_valid policy enforcement.unconstrained_tools must be one of: warn, deny, allow"
                    ),
                }
            }
        }

        if !effective {
            anyhow::bail!("args_valid policy asserts nothing enforced by this evaluator");
        }
        return Ok(());
    }

    validate_schema_map(policy, true, true)
}

fn policy_string_list<'a>(
    value: Option<&'a serde_json::Value>,
    field: &str,
) -> anyhow::Result<Vec<&'a str>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("args_valid policy {field} must be a list"))?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("args_valid policy {field} entries must be strings"))
        })
        .collect()
}

fn is_universal_tool_pattern(pattern: &str) -> bool {
    !pattern.is_empty() && pattern.bytes().all(|byte| byte == b'*')
}

/// Return whether a value carries an unambiguous structured-policy discriminator.
///
/// `schemas` alone is intentionally not a discriminator: the same JSON shape can
/// be a legacy schema map for a tool literally named `schemas`. Current policy
/// documents identify themselves with `version: "2.0"` or another policy field.
pub fn has_structured_args_policy_shape(policy: &serde_json::Value) -> bool {
    let Some(root) = policy.as_object() else {
        return false;
    };
    root.get("version")
        .is_some_and(serde_json::Value::is_string)
        || root.get("name").is_some_and(serde_json::Value::is_string)
        || root.get("allow").is_some_and(serde_json::Value::is_array)
        || root.get("deny").is_some_and(serde_json::Value::is_array)
        || root
            .get("tools")
            .and_then(serde_json::Value::as_object)
            .is_some_and(|tools| {
                [
                    "allow",
                    "deny",
                    "allow_classes",
                    "deny_classes",
                    "approval_required",
                    "approval_required_classes",
                    "restrict_scope",
                    "restrict_scope_classes",
                    "restrict_scope_contract",
                    "redact_args",
                    "redact_args_classes",
                    "redact_args_contract",
                ]
                .iter()
                .any(|key| tools.contains_key(*key))
            })
        || root
            .get("enforcement")
            .and_then(serde_json::Value::as_object)
            .is_some_and(|enforcement| enforcement.contains_key("unconstrained_tools"))
        || [
            "constraints",
            "limits",
            "signatures",
            "tool_pins",
            "discovery",
            "runtime_monitor",
            "kill_switch",
        ]
        .iter()
        .any(|key| root.contains_key(*key))
}

/// What a `sequence_valid` `policy:` file holds, in the shapes it may take.
pub(crate) enum SequencePolicyFile {
    /// A legacy exact-order list of tool names.
    ExactSequence(Vec<String>),
    /// Rules: a bare list, or the `sequences` of a policy-form file.
    Rules(Vec<SequenceRule>),
}

/// Why a `sequence_valid` `policy:` file was not read into rules.
pub(crate) enum SequencePolicyFileError {
    /// A policy-form file that declares aliases, sorted by name.
    DeclaresAliases(Vec<String>),
    /// A policy-form file that declares fields a test does not read, as dotted paths, sorted.
    DeclaresUnreadFields(Vec<String>),
    /// None of the three shapes. Carries the last shape's parse error.
    Invalid(serde_yaml::Error),
}

impl SequencePolicyFileError {
    /// The refusal for a file whose aliases its rules cannot carry. Each reader words the
    /// `Invalid` case itself, as it always has.
    fn alias_refusal(path: &str, names: &[String]) -> anyhow::Error {
        anyhow::anyhow!(
            "sequence_valid policy '{path}' declares aliases ({}), which a test does not apply \
             to its rules; name the member tools in the rules instead",
            names.join(", ")
        )
    }

    /// The refusal for a file that declares fields a test would ignore. The advice after "Remove
    /// them" names where a tool list and `on_error` belong, and only when the file declares one,
    /// since a field the policy form does not have at all needs neither.
    fn unread_fields_refusal(path: &str, fields: &[String]) -> anyhow::Error {
        let mut advice = String::new();
        if fields.iter().any(|field| field.starts_with("tools.")) {
            advice.push_str(", enforce a tool allow or deny list with an args_valid test");
        }
        if fields.iter().any(|field| field == "on_error") {
            advice.push_str(", and set `on_error` on the test or in `settings`");
        }
        anyhow::anyhow!(
            "sequence_valid policy '{path}' declares fields a sequence_valid test does not \
             enforce ({}); the test reads only `sequences`. Remove them{advice}",
            fields.join(", ")
        )
    }

    /// The refusal for content the file declares and a test would not apply, worded once for
    /// every reader. `Err` hands back the parse error of a file that is none of the three shapes,
    /// which each reader words itself, as it always has.
    pub(crate) fn into_refusal(self, path: &str) -> Result<anyhow::Error, serde_yaml::Error> {
        match self {
            Self::DeclaresAliases(names) => Ok(Self::alias_refusal(path, &names)),
            Self::DeclaresUnreadFields(fields) => Ok(Self::unread_fields_refusal(path, &fields)),
            Self::Invalid(err) => Err(err),
        }
    }
}

/// The top-level fields of a policy-form file that a test reads or that only describe the file.
///
/// A test reads `sequences`, and `aliases` is refused on its own above. `version`, `name` and
/// `metadata` assert nothing. Every other field is refused by name rather than listed here, so a
/// field added to `Policy` later is refused until a test enforces it.
const SEQUENCE_POLICY_FILE_READ_FIELDS: &[&str] =
    &["version", "name", "metadata", "sequences", "aliases"];

/// Every field of a policy-form file that a test would ignore, as dotted paths, sorted.
///
/// Read from the document, not from the parsed `Policy`: `on_error` has a default, so a parsed
/// policy cannot tell `on_error: block` from no `on_error` at all. A `tools` mapping is opened
/// so the refusal names the list (`tools.deny`); an empty or null `tools` declares nothing.
/// `as_mapping`, `as_str` and `is_null` look through YAML tags, so a tag hides no field.
fn unread_sequence_policy_fields(source: &str) -> Result<Vec<String>, serde_yaml::Error> {
    let document: serde_yaml::Value = serde_yaml::from_str(source)?;
    let Some(root) = document.as_mapping() else {
        // Both callers reach this only with a mapping. Should that ever change, the file is refused
        // rather than read as declaring nothing.
        return Ok(vec!["<document is not a mapping>".to_string()]);
    };
    let mut unread = Vec::new();
    for (key, value) in root {
        let name = yaml_key_name(key)?;
        if SEQUENCE_POLICY_FILE_READ_FIELDS.contains(&name.as_str()) {
            continue;
        }
        if name == "tools" {
            if value.is_null() {
                continue;
            }
            if let Some(tools) = value.as_mapping() {
                for sub in tools.keys() {
                    unread.push(format!("tools.{}", yaml_key_name(sub)?));
                }
                continue;
            }
        }
        unread.push(name);
    }
    unread.sort_unstable();
    Ok(unread)
}

/// The fields of a sequence policy file that the sequence evaluator does not read, as dotted paths,
/// sorted: every field except `sequences`, `aliases` and the `version`, `name` and `metadata` labels.
///
/// `assay run` refuses a `sequence_valid` `policy:` file that has any (and its `aliases` too);
/// `assay_check_sequence`, which resolves aliases, names them in its answer. Both read the list
/// here, so the two cannot disagree about which fields a sequence check leaves unread.
pub fn sequence_policy_fields_not_read(source: &str) -> anyhow::Result<Vec<String>> {
    Ok(unread_sequence_policy_fields(source)?)
}

/// A mapping key as the refusal names it: the string itself, or its YAML spelling when the key is
/// not a string, so a numeric or otherwise odd key is still named rather than skipped.
fn yaml_key_name(key: &serde_yaml::Value) -> Result<String, serde_yaml::Error> {
    match key.as_str() {
        Some(name) => Ok(name.to_string()),
        None => Ok(serde_yaml::to_string(key)?.trim_end().to_string()),
    }
}

/// Parse a `sequence_valid` `policy:` file, refusing one that declares content a test would not
/// apply: aliases, or any field other than its `sequences` and the fields that describe it.
///
/// The one parse for every reader that turns the file into a test's rules: the contract check
/// (`validate_sequence_policy`), the binding that snapshots it into the test before it runs
/// (`bind_external_expected_inputs`), and `assay migrate`, which writes the rules inline
/// (`config::resolve::resolve_policies`). Each used to parse it separately.
///
/// All three produce rules and nothing else, so an alias would be evaluated as the literal name
/// it is and match no call. A rule whose consequent or forbidden call is an alias would then stop
/// constraining the trace, and the test would pass on calls the rule forbids. Until rules can
/// carry aliases, refusing the file is the only reading that cannot do that.
///
/// The same holds for the rest of the `Policy` document. `assay explain --policy` reads its
/// `tools.allow` and `tools.deny` and blocks the calls they forbid, while a test read only the
/// rules, so `tools: {deny: [rm]}` passed a trace that calls `rm`. A test refuses every such
/// field by name.
pub(crate) fn parse_sequence_policy_file(
    source: &str,
) -> Result<SequencePolicyFile, SequencePolicyFileError> {
    if let Ok(sequence) = serde_yaml::from_str::<Vec<String>>(source) {
        return Ok(SequencePolicyFile::ExactSequence(sequence));
    }
    match serde_yaml::from_str::<super::types::Policy>(source) {
        Ok(policy) => {
            if !policy.aliases.is_empty() {
                let mut names: Vec<String> = policy.aliases.into_keys().collect();
                names.sort_unstable();
                return Err(SequencePolicyFileError::DeclaresAliases(names));
            }
            let unread =
                unread_sequence_policy_fields(source).map_err(SequencePolicyFileError::Invalid)?;
            if !unread.is_empty() {
                return Err(SequencePolicyFileError::DeclaresUnreadFields(unread));
            }
            return Ok(SequencePolicyFile::Rules(policy.sequences));
        }
        // A mapping can only be the policy form; the bare-list parse below never reads one, and its
        // error would describe the document's shape instead of what broke the policy form. A field
        // `Policy` does not have is named like a field a test does not read; anything else keeps the
        // policy form's own parse error.
        Err(policy_error) if is_yaml_mapping(source) => {
            let unread =
                unread_sequence_policy_fields(source).map_err(SequencePolicyFileError::Invalid)?;
            if !unread.is_empty() {
                return Err(SequencePolicyFileError::DeclaresUnreadFields(unread));
            }
            return Err(SequencePolicyFileError::Invalid(policy_error));
        }
        Err(_) => {}
    }
    serde_yaml::from_str::<Vec<SequenceRule>>(source)
        .map(SequencePolicyFile::Rules)
        .map_err(SequencePolicyFileError::Invalid)
}

/// Whether the document is a YAML mapping, looking through a tag on it.
fn is_yaml_mapping(source: &str) -> bool {
    serde_yaml::from_str::<serde_yaml::Value>(source)
        .is_ok_and(|document| document.as_mapping().is_some())
}

/// The contract check's and the binding's wording of a parse failure.
fn read_sequence_policy_rules(path: &str, source: &str) -> anyhow::Result<SequencePolicyFile> {
    parse_sequence_policy_file(source).map_err(|e| {
        e.into_refusal(path)
            .unwrap_or_else(|err| anyhow::anyhow!("invalid sequence_valid policy YAML: {err}"))
    })
}

fn validate_sequence_policy(path: &str) -> anyhow::Result<()> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("failed to read sequence_valid policy '{path}': {e}"))?;
    let rules = match read_sequence_policy_rules(path, &source)? {
        SequencePolicyFile::ExactSequence(_) => return Ok(()),
        SequencePolicyFile::Rules(rules) => rules,
    };
    let expected = Expected::SequenceValid {
        policy: None,
        sequence: None,
        rules: Some(rules),
    };
    if let Some(field) = vacuous_expected_field(&expected) {
        anyhow::bail!("`{field}` asserts nothing");
    }
    if let Some(reason) = non_executable_expected_reason(&expected) {
        anyhow::bail!("expected block is not executable: {reason}");
    }
    if let Some(reason) = ineffective_expected_reason(&expected) {
        anyhow::bail!("{reason}");
    }
    Ok(())
}

fn validate_schema_map(
    value: &serde_json::Value,
    reject_root_schema_keywords: bool,
    require_effective_schema: bool,
) -> anyhow::Result<()> {
    let schemas = value
        .as_object()
        .filter(|schemas| !schemas.is_empty())
        .ok_or_else(|| anyhow::anyhow!("schema must be a non-empty tool-name-to-schema map"))?;
    if reject_root_schema_keywords && schemas.keys().any(|key| is_json_schema_keyword(key)) {
        anyhow::bail!(
            "root JSON Schema keywords cannot be used as tool names; expected a tool-name-to-schema map"
        );
    }
    if require_effective_schema && schema_map_asserts_nothing(value) {
        anyhow::bail!("schema map asserts nothing");
    }
    for (tool, schema) in schemas {
        if !schema.is_object() && !schema.is_boolean() {
            anyhow::bail!(
                "schema entry '{tool}' must be a JSON Schema; expected a tool-name-to-schema map"
            );
        }
        crate::policy_engine::compile_schema(schema)
            .map_err(|e| anyhow::anyhow!("schema for tool '{tool}' failed to compile: {e}"))?;
        if let Some(reason) = tool_schema_refusal(tool, schema) {
            anyhow::bail!("schema for tool '{tool}' {reason}");
        }
    }
    Ok(())
}

/// Why one tool's schema constrains less than it reads, or `None`.
///
/// Each tool's schema is judged on its own. The policy-level rule ("asserts nothing") answers for
/// the map as a whole, so a deny list, `enforcement.unconstrained_tools: deny` or another tool's
/// schema made a policy effective while one tool's schema constrained nothing, and the call it was
/// written for passed. Two shapes are refused, checked on the schema's root:
///
/// - a root key the evaluator does not read: one that is not a JSON Schema keyword, or a draft-3
///   name the evaluator ignores (`divisibleBy`, `disallow`, `extends`). A key starting with `x-`
///   is an annotation and is kept. This catches a map of argument names to constraints, alone or
///   beside `type: object`, which reads as constraints and is none.
/// - a schema that asserts nothing and does not say so: anything but `true`, or an object of only
///   annotation or identifier keywords (`{}` included), which accept any value on purpose.
///
/// The root is the position checked. Unread keys below it (inside `properties`, say) sit where
/// generators put annotations the evaluator rightly ignores; a schema that has them and asserts
/// nothing is still caught by the second rule.
pub(crate) fn tool_schema_refusal(tool: &str, schema: &serde_json::Value) -> Option<String> {
    if let Some(object) = schema.as_object() {
        let mut unread: Vec<&str> = object
            .keys()
            .map(String::as_str)
            .filter(|key| !key.starts_with("x-"))
            .filter(|key| !is_json_schema_keyword(key) || is_draft3_only_keyword(key))
            .collect();
        if !unread.is_empty() {
            unread.sort_unstable();
            let mut reason = format!(
                "has keys this evaluator does not read, so they constrain nothing: {}.",
                unread.join(", ")
            );
            if let Some(argument) = unread.iter().find(|key| !is_draft3_only_keyword(key)) {
                reason.push_str(&format!(
                    " A map of argument names to constraints is not a JSON Schema: describe the \
                     arguments under `properties`, as `{{type: object, properties: \
                     {{{argument}: <constraint>}}}}`."
                ));
            }
            if unread.iter().any(|key| is_draft3_only_keyword(key)) {
                reason.push_str(
                    " Write a draft-3 keyword in its current form: `allOf` for `extends`, \
                     `multipleOf` for `divisibleBy`, `not` for `disallow`.",
                );
            }
            reason.push_str(" Prefix a key that is only an annotation with `x-`");
            return Some(reason);
        }
    }
    if schema_asserts_nothing(schema) && !accepts_any_value_on_purpose(schema) {
        return Some(format!(
            "asserts nothing, so it accepts any value. Write `{tool}: true` to accept any value on \
             purpose, or constrain it with JSON Schema keywords such as `type`, `required` and \
             `pattern`"
        ));
    }
    None
}

/// Draft-3 keywords the evaluator does not implement; `is_json_schema_keyword` lists them so a
/// tool cannot be named after one, but in a schema they constrain nothing.
fn is_draft3_only_keyword(key: &str) -> bool {
    matches!(key, "divisibleBy" | "disallow" | "extends")
}

/// `true`, or an object whose every key is an annotation or identifier keyword or an `x-`
/// annotation (`{}` included): a schema written to accept any value. `$defs` and `definitions`
/// count, because shared `$defs` are merged into every object tool schema before this check.
fn accepts_any_value_on_purpose(schema: &serde_json::Value) -> bool {
    match schema {
        serde_json::Value::Bool(value) => *value,
        serde_json::Value::Object(object) => object.keys().all(|key| {
            key.starts_with("x-")
                || matches!(
                    key.as_str(),
                    "title"
                        | "description"
                        | "$comment"
                        | "default"
                        | "examples"
                        | "deprecated"
                        | "readOnly"
                        | "writeOnly"
                        | "$schema"
                        | "$id"
                        | "id"
                        | "$anchor"
                        | "$dynamicAnchor"
                        | "$recursiveAnchor"
                        | "$vocabulary"
                        | "$defs"
                        | "definitions"
                )
        }),
        _ => false,
    }
}

fn is_json_schema_keyword(key: &str) -> bool {
    matches!(
        key,
        "$schema"
            | "$id"
            | "$ref"
            | "$defs"
            | "$anchor"
            | "$dynamicRef"
            | "$dynamicAnchor"
            | "$vocabulary"
            | "$comment"
            | "id"
            | "definitions"
            | "dependencies"
            | "additionalItems"
            | "$recursiveRef"
            | "$recursiveAnchor"
            | "divisibleBy"
            | "disallow"
            | "extends"
            | "type"
            | "enum"
            | "const"
            | "multipleOf"
            | "maximum"
            | "exclusiveMaximum"
            | "minimum"
            | "exclusiveMinimum"
            | "maxLength"
            | "minLength"
            | "pattern"
            | "items"
            | "prefixItems"
            | "contains"
            | "maxItems"
            | "minItems"
            | "uniqueItems"
            | "maxContains"
            | "minContains"
            | "properties"
            | "patternProperties"
            | "additionalProperties"
            | "propertyNames"
            | "maxProperties"
            | "minProperties"
            | "required"
            | "dependentRequired"
            | "dependentSchemas"
            | "unevaluatedItems"
            | "unevaluatedProperties"
            | "allOf"
            | "anyOf"
            | "oneOf"
            | "not"
            | "if"
            | "then"
            | "else"
            | "title"
            | "description"
            | "default"
            | "deprecated"
            | "readOnly"
            | "writeOnly"
            | "examples"
            | "format"
            | "contentEncoding"
            | "contentMediaType"
            | "contentSchema"
    )
}

/// Validate the execution contract while preserving omitted-`expected` compatibility.
pub(crate) fn validate_test_case_for_execution(test: &TestCase) -> anyhow::Result<()> {
    // Deserialization represents an omitted `expected:` key with this exact default.
    // A written empty block never reaches here because the parser rejects it. Keep
    // the historical warning-only behavior until the assertion contract is tightened.
    if matches!(
        &test.expected,
        Expected::MustContain { must_contain } if must_contain.is_empty()
    ) {
        return Ok(());
    }
    validate_expected_for_execution(&test.expected)
}

/// True for the legacy empty-`must_contain` sentinel.
///
/// `TestCase` does not retain whether this shape came from an omitted key or from
/// programmatic construction, so serialization cannot distinguish those origins.
pub(crate) fn is_omitted_expected_sentinel(e: &Expected) -> bool {
    matches!(e, Expected::MustContain { must_contain } if must_contain.is_empty())
}

pub(crate) fn default_one() -> u32 {
    1
}

pub(crate) fn default_min_score() -> f64 {
    0.80
}

impl EvalConfig {
    pub fn is_legacy(&self) -> bool {
        self.version == 0
    }

    pub fn has_legacy_usage(&self) -> bool {
        self.tests
            .iter()
            .any(|t: &TestCase| t.expected.get_policy_path().is_some())
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.version >= 1 {
            for test in &self.tests {
                if matches!(test.expected, Expected::Reference { .. }) {
                    anyhow::bail!("$ref in expected block is not allowed in configVersion >= 1. Run `assay migrate` to inline policies.");
                }
            }
        }
        Ok(())
    }

    /// Get the effective error policy for a test.
    /// Test-level on_error overrides suite-level settings.
    pub fn effective_error_policy(&self, test: &TestCase) -> ErrorPolicy {
        test.on_error.unwrap_or(self.settings.on_error)
    }
}

impl Expected {
    pub fn get_policy_path(&self) -> Option<&str> {
        #[expect(
            clippy::wildcard_enum_match_arm,
            reason = "only ArgsValid and SequenceValid carry a policy path"
        )]
        match self {
            Expected::ArgsValid { policy, .. } => policy.as_deref(),
            Expected::SequenceValid { policy, .. } => policy.as_deref(),
            _ => None,
        }
    }

    /// Per-test thresholding for baseline regression (mode/max_drop) when this Expected variant matches the metric.
    pub fn thresholding_for_metric(&self, metric_name: &str) -> Option<&ThresholdingConfig> {
        match (metric_name, self) {
            ("semantic_similarity_to", Expected::SemanticSimilarityTo { thresholding, .. }) => {
                thresholding.as_ref()
            }
            ("faithfulness", Expected::Faithfulness { thresholding, .. }) => thresholding.as_ref(),
            ("relevance", Expected::Relevance { thresholding, .. }) => thresholding.as_ref(),
            _ => None,
        }
    }
}

impl TestStatus {
    pub fn parse(s: &str) -> Self {
        match s {
            "pass" => TestStatus::Pass,
            "fail" => TestStatus::Fail,
            "flaky" => TestStatus::Flaky,
            "warn" => TestStatus::Warn,
            "error" => TestStatus::Error,
            "skipped" => TestStatus::Skipped,
            "unstable" => TestStatus::Unstable,
            "allowed_on_error" => TestStatus::AllowedOnError,
            _ => TestStatus::Error,
        }
    }

    /// Returns true if this status should be treated as passing for CI purposes
    pub fn is_passing(&self) -> bool {
        matches!(
            self,
            TestStatus::Pass | TestStatus::AllowedOnError | TestStatus::Warn
        )
    }

    /// Returns true if this status should block CI
    pub fn is_blocking(&self) -> bool {
        matches!(self, TestStatus::Fail | TestStatus::Error)
    }
}
