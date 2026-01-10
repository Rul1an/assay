use super::jsonrpc::{
    ContentItem, JsonRpcRequest, JsonRpcResponse, ToolCallResult, ToolResultBody,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, OnceLock};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpPolicy {
    #[serde(default)]
    pub version: String,

    #[serde(default)]
    pub name: String,

    #[serde(default)]
    pub tools: ToolPolicy,

    // Legacy v1: root-level allow/deny (normalized into tools.* on load)
    #[serde(default)]
    pub allow: Option<Vec<String>>,
    #[serde(default)]
    pub deny: Option<Vec<String>>,

    /// V2: JSON Schema per tool (primary)
    #[serde(default)]
    pub schemas: HashMap<String, Value>,

    /// V1 (deprecated): Regex constraints - auto-converted to schemas on load
    #[serde(default, deserialize_with = "deserialize_constraints")]
    pub constraints: Vec<ConstraintRule>,

    #[serde(default)]
    pub enforcement: EnforcementSettings,

    #[serde(default)]
    pub limits: Option<GlobalLimits>,

    #[serde(default)]
    pub signatures: Option<SignaturePolicy>,

    /// Compiled schemas (lazy, thread-safe, shared across clones)
    #[serde(skip)]
    pub(crate) compiled: Arc<OnceLock<HashMap<String, Arc<jsonschema::JSONSchema>>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnforcementSettings {
    /// What to do when a tool has no schema
    #[serde(default = "default_unconstrained")]
    pub unconstrained_tools: UnconstrainedMode,
}

impl Default for EnforcementSettings {
    fn default() -> Self {
        Self {
            unconstrained_tools: UnconstrainedMode::Warn,
        }
    }
}

fn default_unconstrained() -> UnconstrainedMode {
    UnconstrainedMode::Warn
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum UnconstrainedMode {
    #[default]
    Warn,
    Deny,
    Allow,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SignaturePolicy {
    #[serde(default)]
    pub check_descriptions: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalLimits {
    pub max_requests_total: Option<u64>,
    pub max_tool_calls_total: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolPolicy {
    pub allow: Option<Vec<String>>,
    pub deny: Option<Vec<String>>,
}

// Canonical Rule Shape (Legacy V1)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintRule {
    pub tool: String,
    pub params: BTreeMap<String, ConstraintParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintParam {
    #[serde(default)]
    pub matches: Option<String>,
}

#[derive(Debug, Default)]
pub struct PolicyState {
    pub requests_count: u64,
    pub tool_calls_count: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PolicyDecision {
    Allow,
    AllowWithWarning {
        tool: String,
        code: String,
        reason: String,
    },
    Deny {
        tool: String,
        code: String,
        reason: String,
        contract: Value,
    },
}

// Dual-Shape Deserializer Helper (Legacy)
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ConstraintsCompat {
    List(Vec<ConstraintRule>),
    Map(BTreeMap<String, BTreeMap<String, InputParamConstraint>>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum InputParamConstraint {
    Direct(String),
    Object(ConstraintParam),
}

fn deserialize_constraints<'de, D>(d: D) -> Result<Vec<ConstraintRule>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let c = Option::<ConstraintsCompat>::deserialize(d)?;
    let out = match c {
        None => vec![],
        Some(ConstraintsCompat::List(v)) => v,
        Some(ConstraintsCompat::Map(m)) => m
            .into_iter()
            .map(|(tool, params)| {
                let new_params = params
                    .into_iter()
                    .map(|(arg, val)| {
                        let param = match val {
                            InputParamConstraint::Direct(s) => ConstraintParam { matches: Some(s) },
                            InputParamConstraint::Object(o) => o,
                        };
                        (arg, param)
                    })
                    .collect();
                ConstraintRule {
                    tool,
                    params: new_params,
                }
            })
            .collect(),
    };
    Ok(out)
}

fn matches_tool_pattern(tool_name: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return tool_name == pattern;
    }
    let starts_star = pattern.starts_with('*');
    let ends_star = pattern.ends_with('*');
    match (starts_star, ends_star) {
        (true, true) => {
            let inner = pattern.trim_matches('*');
            if inner.is_empty() {
                true
            } else {
                tool_name.contains(inner)
            }
        }
        (false, true) => {
            let prefix = pattern.trim_end_matches('*');
            !prefix.is_empty() && tool_name.starts_with(prefix)
        }
        (true, false) => {
            let suffix = pattern.trim_start_matches('*');
            !suffix.is_empty() && tool_name.ends_with(suffix)
        }
        (false, false) => tool_name == pattern,
    }
}

impl McpPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_file(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let mut policy: McpPolicy = serde_yaml::from_str(&content)?;

        // Check for v1 format and warn if necessary
        if policy.is_v1_format() {
            if std::env::var("ASSAY_STRICT_DEPRECATIONS").ok().as_deref() == Some("1") {
                anyhow::bail!("Strict mode: v1 policy format (constraints) is not allowed.");
            }
            emit_deprecation_warning();
        }

        // Normalize legacy shapes
        policy.normalize_legacy_shapes();

        // Auto-migrate v1 constraints
        if !policy.constraints.is_empty() {
            policy.migrate_constraints_to_schemas();
        }

        Ok(policy)
    }

    pub fn is_v1_format(&self) -> bool {
        // v1 if constraints are present OR version is explicitly "1.0"
        !self.constraints.is_empty() || self.version == "1.0"
    }

    /// Normalize legacy root-level allow/deny into tools.allow/deny.
    pub fn normalize_legacy_shapes(&mut self) {
        if let Some(allow) = self.allow.take() {
            let mut current = self.tools.allow.take().unwrap_or_default();
            current.extend(allow);
            self.tools.allow = Some(current);
        }
        if let Some(deny) = self.deny.take() {
            let mut current = self.tools.deny.take().unwrap_or_default();
            current.extend(deny);
            self.tools.deny = Some(current);
        }
    }

    /// Migrate V1 regex constraints to V2 JSON Schemas.
    /// Warning: This clears the `constraints` field.
    pub fn migrate_constraints_to_schemas(&mut self) {
        for constraint in std::mem::take(&mut self.constraints) {
            let schema = constraint_to_schema(&constraint);
            self.schemas.insert(constraint.tool.clone(), schema);
        }
        if self.version.is_empty() || self.version == "1.0" {
            self.version = "2.0".to_string();
        }
    }

    fn compiled_schemas(&self) -> &HashMap<String, Arc<jsonschema::JSONSchema>> {
        self.compiled.get_or_init(|| self.compile_all_schemas())
    }

    pub fn compile_all_schemas(&self) -> HashMap<String, Arc<jsonschema::JSONSchema>> {
        // Option 1: Inline $defs into every schema to support relative #/$defs/... refs
        let root_defs = self.schemas.get("$defs").cloned();

        let mut compiled = HashMap::new();
        for (tool_name, schema) in &self.schemas {
            if tool_name.starts_with('$') {
                continue;
            }

            let mut schema_to_compile = schema.clone();
            // Inject $defs if they exist and the schema is an object
            if let Some(defs) = &root_defs {
                if let Value::Object(map) = &mut schema_to_compile {
                    // Only insert if not already present to allow overrides (or just overwrite?)
                    // For now, insert if missing or overwrite to ensure global defs availability.
                    map.insert("$defs".to_string(), defs.clone());
                }
            }

            match jsonschema::JSONSchema::compile(&schema_to_compile) {
                Ok(validator) => {
                    compiled.insert(tool_name.clone(), Arc::new(validator));
                }
                Err(e) => {
                    tracing::error!("Failed to compile schema for tool {}: {}", tool_name, e);
                    // Fail securely: do not allow tools with broken schemas to load.
                    panic!(
                        "Failed to compile JSON schema for tool '{}': {}",
                        tool_name, e
                    );
                }
            }
        }
        compiled
    }

    /// Single evaluation entry point for CLI and Server
    pub fn evaluate(
        &self,
        tool_name: &str,
        args: &Value,
        state: &mut PolicyState,
    ) -> PolicyDecision {
        // 1. Rate limits
        if let Some(decision) = self.check_rate_limits(state) {
            return decision;
        }

        // 2. Deny list
        if self.is_denied(tool_name) {
            return PolicyDecision::Deny {
                tool: tool_name.to_string(),
                code: "E_TOOL_DENIED".to_string(),
                reason: "Tool is explicitly denylisted".to_string(),
                contract: self.format_deny_contract(
                    tool_name,
                    "E_TOOL_DENIED",
                    "Tool is denylisted",
                ),
            };
        }

        // 3. Allow list
        if self.has_allowlist() && !self.is_allowed(tool_name) {
            return PolicyDecision::Deny {
                tool: tool_name.to_string(),
                code: "E_TOOL_NOT_ALLOWED".to_string(),
                reason: "Tool is not in the allowlist".to_string(),
                contract: self.format_deny_contract(
                    tool_name,
                    "E_TOOL_NOT_ALLOWED",
                    "Tool is not in allowlist",
                ),
            };
        }

        // 4. Schema Validation
        let compiled = self.compiled_schemas();
        if let Some(validator) = compiled.get(tool_name) {
            match validator.validate(args) {
                Ok(_) => return PolicyDecision::Allow,
                Err(errors) => {
                    let violations: Vec<_> = errors
                        .map(|e| {
                            json!({
                                "path": e.instance_path.to_string(),
                                "message": e.to_string(),
                            })
                        })
                        .collect();
                    return PolicyDecision::Deny {
                        tool: tool_name.to_string(),
                        code: "E_ARG_SCHEMA".to_string(),
                        reason: "JSON Schema validation failed".to_string(),
                        contract: json!({
                            "status": "deny",
                            "error_code": "E_ARG_SCHEMA",
                            "tool": tool_name,
                            "violations": violations,
                        }),
                    };
                }
            }
        }

        // 5. Unconstrained Mode
        match self.enforcement.unconstrained_tools {
            UnconstrainedMode::Deny => PolicyDecision::Deny {
                tool: tool_name.to_string(),
                code: "E_TOOL_UNCONSTRAINED".to_string(),
                reason: "Tool has no schema (enforcement: deny)".to_string(),
                contract: self.format_deny_contract(
                    tool_name,
                    "E_TOOL_UNCONSTRAINED",
                    "Tool has no schema (enforcement: deny)",
                ),
            },
            UnconstrainedMode::Warn => PolicyDecision::AllowWithWarning {
                tool: tool_name.to_string(),
                code: "E_TOOL_UNCONSTRAINED".to_string(),
                reason: "Tool allowed but has no schema".to_string(),
            },
            UnconstrainedMode::Allow => PolicyDecision::Allow,
        }
    }

    // Helper methods (extracted from original code or refactored)
    fn check_rate_limits(&self, state: &mut PolicyState) -> Option<PolicyDecision> {
        state.requests_count += 1;
        state.tool_calls_count += 1; // Simplified: Assumes evaluate called on tool call

        if let Some(limits) = &self.limits {
            if let Some(max) = limits.max_requests_total {
                // Note: requests_count tracks total JSON-RPC, which we might not have here accurately
                // unless state is persistent session state.
                // For now, allow it to increment, assuming state is managing session.
                if state.requests_count > max {
                    return Some(PolicyDecision::Deny {
                        tool: "ALL".to_string(),
                        code: "E_RATE_LIMIT".to_string(),
                        reason: "Rate limit exceeded (total requests)".to_string(),
                        contract: json!({ "status": "deny", "error_code": "E_RATE_LIMIT" }),
                    });
                }
            }

            if let Some(max) = limits.max_tool_calls_total {
                if state.tool_calls_count > max {
                    return Some(PolicyDecision::Deny {
                        tool: "ALL".to_string(),
                        code: "E_RATE_LIMIT".to_string(),
                        reason: "Rate limit exceeded (tool calls)".to_string(),
                        contract: json!({ "status": "deny", "error_code": "E_RATE_LIMIT" }),
                    });
                }
            }
        }
        None
    }

    fn is_denied(&self, tool_name: &str) -> bool {
        let root_deny = self.deny.as_ref();
        let tools_deny = self.tools.deny.as_ref();
        root_deny
            .iter()
            .flat_map(|v| v.iter())
            .chain(tools_deny.iter().flat_map(|v| v.iter()))
            .any(|pattern| matches_tool_pattern(tool_name, pattern))
    }

    fn has_allowlist(&self) -> bool {
        self.allow.is_some() || self.tools.allow.is_some()
    }

    fn is_allowed(&self, tool_name: &str) -> bool {
        let root_allow = self.allow.as_ref();
        let tools_allow = self.tools.allow.as_ref();
        root_allow
            .iter()
            .flat_map(|v| v.iter())
            .chain(tools_allow.iter().flat_map(|v| v.iter()))
            .any(|pattern| matches_tool_pattern(tool_name, pattern))
    }

    fn format_deny_contract(&self, tool: &str, code: &str, reason: &str) -> Value {
        json!({
            "status": "deny",
            "error_code": code,
            "tool": tool,
            "reason": reason
        })
    }

    // Proxy-specific check method (Legacy compatibility wrapper)
    pub fn check(&self, request: &JsonRpcRequest, state: &mut PolicyState) -> PolicyDecision {
        if !request.is_tool_call() {
            state.requests_count += 1;
            return PolicyDecision::Allow;
        }
        if let Some(params) = request.tool_params() {
            // evaluate() increments counts, so we don't need to increment requests_count here
            self.evaluate(&params.name, &params.arguments, state)
        } else {
            // Ordinary request, just count it
            state.requests_count += 1;
            PolicyDecision::Allow
        }
    }
}

fn constraint_to_schema(constraint: &ConstraintRule) -> Value {
    let mut properties = json!({});
    let mut required = vec![];

    for (param_name, param_constraint) in &constraint.params {
        if let Some(pattern) = &param_constraint.matches {
            properties[param_name] = json!({
                "type": "string",
                "pattern": pattern,
                "minLength": 1
                // No maxLength restriction for V1 backward compatibility
            });
            required.push(param_name.clone());
        }
    }

    json!({
        "type": "object",
        // Allow additional properties for V1 backward compatibility
        "additionalProperties": true,
        "properties": properties,
        "required": required,
    })
}

pub fn make_deny_response(id: Value, msg: &str, contract: Value) -> String {
    let body = ToolResultBody {
        content: vec![ContentItem::Text {
            text: msg.to_string(),
        }],
        is_error: true,
        structured_content: Some(contract),
    };
    let resp = JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        payload: ToolCallResult { result: body },
    };
    serde_json::to_string(&resp).unwrap_or_default() + "\n"
}

fn emit_deprecation_warning() {
    static WARNED: OnceLock<()> = OnceLock::new();
    WARNED.get_or_init(|| {
        eprintln!(
            "\n\x1b[33m⚠️  DEPRECATED: v1 policy format detected\x1b[0m\n\
             \x1b[33m   The 'constraints:' syntax is deprecated and will be removed in Assay v2.0.0.\x1b[0m\n\
             \x1b[33m   Migrate now:\x1b[0m\n\
             \x1b[33m     assay policy migrate --input <file>\x1b[0m\n\
             \x1b[33m   See: https://docs.assay.dev/migration/v1-to-v2\x1b[0m\n"
        );
    });
}
