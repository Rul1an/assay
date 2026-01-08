# ADR 001: Unify Policy Engines (JSON Schema vs Regex)

**Status:** Accepted  
**Date:** 2026-01-07  
**Authors:** Antigravity, Roel Schuurkes  
**Target Version:** Assay v1.6.0  

---

## 1. Context

Assay v1.5.1 maintains two divergent policy execution engines, causing user confusion and tooling incompatibility.

### Engine A: Core Engine (CLI)
| Aspect | Detail |
|--------|--------|
| Location | `crates/assay-core/src/mcp/policy.rs` |
| Struct | `McpPolicy` with `ConstraintRule` |
| Capabilities | Allow/deny lists, wildcards, rate limits, signatures, SARIF output |
| Constraint Logic | Custom **Regex** matching |
| Used By | `assay coverage`, `assay run` |

### Engine B: Server Engine (Runtime)
| Aspect | Detail |
|--------|--------|
| Location | `crates/assay-core/src/policy_engine.rs` |
| Struct | Raw `serde_json::Value` |
| Capabilities | JSON Schema validation only |
| Used By | `assay-mcp-server`, `assay_check_args` tool |

### The Problem

Users cannot use the same policy file for both CI analysis (`assay coverage`) and runtime protection (`assay-mcp-server`).

---

## 2. Decision

**Standardize on JSON Schema for argument constraints; unify the evaluation pipeline.**

### Key Clarification
JSON Schema replaces regex as the *argument constraint language*. The policy engine remains responsible for:
- Tool allow/deny filtering (with wildcards)
- Rate limits
- Signature/description checks
- Contract formatting (SARIF, agentic suggestions)
- Error codes and decisions

---

## 3. Versioning Scheme

| Concept | Value | Meaning |
|---------|-------|---------|
| **Policy schema version** | `"2.0"` | Format of the YAML policy file |
| **Assay v1.6.0** | Release | Introduces policy v2.0, full backward compat |
| **Assay v1.7.0** | Release | v1.x policies still work, hard deprecation warnings |
| **Assay v2.0.0** | Release | v1.x policy support removed (breaking change) |

**Rule:** Breaking changes only in major versions.

---

## 4. Unified Policy Format (v2.0)

```yaml
# policy.yaml
version: "2.0"
name: "production-security"

metadata:
  description: "Hardened policy for production MCP servers"
  author: "security-team"
  cve_coverage: ["CVE-2025-53109", "CVE-2025-53967"]

# Tool filtering (unchanged from v1.x)
tools:
  allow: ["read_file", "list_directory", "search_files"]
  deny: ["create_symlink", "write_file", "execute_*"]  # Wildcards supported

# NEW: JSON Schema per tool (replaces constraints)
schemas:
  read_file:
    type: object
    additionalProperties: false          # Security default
    properties:
      path:
        type: string
        pattern: "^/workspace/.*"
        minLength: 1
        maxLength: 4096
    required: ["path"]

  list_directory:
    type: object
    additionalProperties: false
    properties:
      path:
        type: string
        pattern: "^/workspace/.*"
    required: ["path"]

# NEW: Enforcement settings
enforcement:
  unconstrained_tools: warn              # warn | deny | allow
  # - warn: Allow but emit E_TOOL_UNCONSTRAINED warning (default)
  # - deny: Block tools without schema (hardened mode)
  # - allow: Silent allow (legacy behavior)

# Rate limits (unchanged)
limits:
  max_requests_total: 1000
  max_tool_calls_total: 500

# Signature checks (unchanged)
signatures:
  check_descriptions: true
```

### 4.1 Wildcard Support

Wildcard patterns in `tools.allow` and `tools.deny` support:
- `prefix*` — matches tools starting with prefix
- `*suffix` — matches tools ending with suffix  
- `*contains*` — matches tools containing substring
- `*` — matches all tools

**Limitation:** Glob-in-the-middle patterns like `foo*bar` are not supported.

### 4.2 Enforcement Modes

| Mode | Behavior | Use Case |
|------|----------|----------|
| `warn` | Allow + emit `E_TOOL_UNCONSTRAINED` | Default, development |
| `deny` | Block unconstrained tools | Production hardened |
| `allow` | Silent allow | Legacy compat |

**Default:** `warn` for both CLI and Server (consistent behavior).

### 4.3 Reserved Keys

Keys under `schemas` starting with `$` are reserved for JSON Schema meta-sections (e.g., `$defs`) and cannot be used as tool names.

---

## 5. Error Codes (Canonical)

| Code | Meaning | When |
|------|---------|------|
| `E_TOOL_DENIED` | Tool in deny list | `tools.deny` match |
| `E_TOOL_NOT_ALLOWED` | Tool not in allow list | `tools.allow` defined, no match |
| `E_ARG_SCHEMA` | JSON Schema validation failed | Schema violation |
| `E_TOOL_UNCONSTRAINED` | Tool allowed but no schema | `enforcement.unconstrained_tools: warn` |
| `E_RATE_LIMIT` | Rate limit exceeded | `limits.*` exceeded |
| `E_POLICY_INVALID` | Policy file malformed | Parse error, invalid regex, JSON Schema compilation failure |

**Note:** The policy engine emits `E_*` codes. CLI diagnostics map these to existing diagnostic code families (e.g., `E_CFG_*`, `E_PATH_*`) where appropriate for consistent user-facing output.

---

## 6. Implementation Plan

### Phase 1: Core Unification (Week 1-2)

#### 6.1 Unified `McpPolicy` Struct

```rust
// crates/assay-core/src/mcp/policy.rs

use std::sync::{Arc, OnceLock};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpPolicy {
    #[serde(default)]
    pub version: String,
    
    #[serde(default)]
    pub name: String,
    
    #[serde(default)]
    pub metadata: Option<PolicyMetadata>,
    
    #[serde(default)]
    pub tools: ToolPolicy,
    
    // Legacy v1: root-level allow/deny (normalized into tools.* on load)
    #[serde(default)]
    allow: Option<Vec<String>>,
    #[serde(default)]
    deny: Option<Vec<String>>,
    
    /// V2: JSON Schema per tool (primary)
    #[serde(default)]
    pub schemas: HashMap<String, Value>,
    
    /// V1 (deprecated): Regex constraints - auto-converted to schemas on load
    #[serde(default, deserialize_with = "deserialize_constraints")]
    constraints: Vec<ConstraintRule>,
    
    #[serde(default)]
    pub enforcement: EnforcementSettings,
    
    #[serde(default)]
    pub limits: Option<GlobalLimits>,
    
    #[serde(default)]
    pub signatures: Option<SignaturePolicy>,
    
    /// Compiled schemas (lazy, thread-safe) - compiled against full policy doc for $ref resolution
    #[serde(skip)]
    compiled: OnceLock<HashMap<String, Arc<jsonschema::JSONSchema>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnforcementSettings {
    /// What to do when a tool has no schema
    #[serde(default = "default_unconstrained")]
    pub unconstrained_tools: UnconstrainedMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum UnconstrainedMode {
    #[default]
    Warn,
    Deny,
    Allow,
}

fn default_unconstrained() -> UnconstrainedMode {
    UnconstrainedMode::Warn
}

impl Default for EnforcementSettings {
    fn default() -> Self {
        Self { unconstrained_tools: UnconstrainedMode::Warn }
    }
}
```

#### 6.2 Schema Compilation (All-at-Once)

```rust
impl McpPolicy {
    /// Load policy and normalize legacy shapes
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let mut policy: McpPolicy = serde_yaml::from_str(&content)?;
        
        // Normalize legacy root-level allow/deny into tools.*
        policy.normalize_legacy_shapes();
        
        // Auto-migrate v1 constraints to schemas
        if !policy.constraints.is_empty() {
            tracing::warn!(
                "Deprecated v1.x constraints detected. Run `assay policy migrate {}` to update.",
                path.display()
            );
            policy.migrate_constraints_to_schemas();
        }
        
        Ok(policy)
    }
    
    /// Normalize v1 root-level allow/deny into tools.allow/tools.deny
    fn normalize_legacy_shapes(&mut self) {
        if let Some(allow) = self.allow.take() {
            self.tools.allow = Some(
                self.tools.allow.take().unwrap_or_default()
                    .into_iter().chain(allow).collect()
            );
        }
        if let Some(deny) = self.deny.take() {
            self.tools.deny = Some(
                self.tools.deny.take().unwrap_or_default()
                    .into_iter().chain(deny).collect()
            );
        }
    }
    
    /// Get compiled schemas - compiles all at once on first access.
    /// Compilation uses full policy document as root for $ref resolution.
    fn compiled_schemas(&self) -> &HashMap<String, Arc<jsonschema::JSONSchema>> {
        self.compiled.get_or_init(|| {
            self.compile_all_schemas()
        })
    }
    
    /// Compile all schemas with access to full policy document for $ref resolution.
    fn compile_all_schemas(&self) -> HashMap<String, Arc<jsonschema::JSONSchema>> {
        // Build root document that includes $defs for $ref resolution
        let root_doc = json!({
            "$defs": self.schemas.get("$defs").cloned().unwrap_or(json!({})),
            "schemas": &self.schemas,
        });
        
        let mut compiled = HashMap::new();
        
        for (tool_name, schema) in &self.schemas {
            // Skip meta-keys ($defs, etc.)
            if tool_name.starts_with('$') {
                continue;
            }
            
            // Compile with root document context for $ref resolution
            match jsonschema::JSONSchema::options()
                .with_document(root_doc.clone())
                .compile(schema)
            {
                Ok(validator) => {
                    compiled.insert(tool_name.clone(), Arc::new(validator));
                }
                Err(e) => {
                    tracing::error!(
                        tool = %tool_name,
                        error = %e,
                        "Failed to compile JSON Schema for tool"
                    );
                    // Skip invalid schemas - they'll fail at runtime with E_POLICY_INVALID
                }
            }
        }
        
        compiled
    }
}
```

#### 6.3 Unified Evaluation Pipeline

```rust
impl McpPolicy {
    /// Single evaluation entry point for CLI and Server
    pub fn evaluate(&self, tool_name: &str, args: &Value, state: &mut PolicyState) -> PolicyDecision {
        // 1. Rate limits
        if let Some(decision) = self.check_rate_limits(state) {
            return decision;
        }
        
        // 2. Deny list (with wildcards)
        if self.is_denied(tool_name) {
            return PolicyDecision::Deny {
                tool: tool_name.to_string(),
                code: "E_TOOL_DENIED".to_string(),
                reason: "Tool is explicitly denylisted".to_string(),
                contract: self.format_deny_contract(tool_name, "E_TOOL_DENIED"),
            };
        }
        
        // 3. Allow list (if defined)
        if self.has_allowlist() && !self.is_allowed(tool_name) {
            return PolicyDecision::Deny {
                tool: tool_name.to_string(),
                code: "E_TOOL_NOT_ALLOWED".to_string(),
                reason: "Tool is not in the allowlist".to_string(),
                contract: self.format_deny_contract(tool_name, "E_TOOL_NOT_ALLOWED"),
            };
        }
        
        // 4. Schema validation (JSON Schema)
        let compiled = self.compiled_schemas();
        if let Some(validator) = compiled.get(tool_name) {
            return self.evaluate_schema(tool_name, validator, args);
        }
        
        // 5. No schema defined - check enforcement mode
        match self.enforcement.unconstrained_tools {
            UnconstrainedMode::Deny => PolicyDecision::Deny {
                tool: tool_name.to_string(),
                code: "E_TOOL_UNCONSTRAINED".to_string(),
                reason: "Tool has no schema (enforcement: deny)".to_string(),
                contract: self.format_deny_contract(tool_name, "E_TOOL_UNCONSTRAINED"),
            },
            UnconstrainedMode::Warn => PolicyDecision::AllowWithWarning {
                tool: tool_name.to_string(),
                code: "E_TOOL_UNCONSTRAINED".to_string(),
                reason: "Tool allowed but has no schema".to_string(),
            },
            UnconstrainedMode::Allow => PolicyDecision::Allow,
        }
    }
    
    fn evaluate_schema(
        &self, 
        tool: &str, 
        validator: &jsonschema::JSONSchema, 
        args: &Value
    ) -> PolicyDecision {
        match validator.validate(args) {
            Ok(_) => PolicyDecision::Allow,
            Err(errors) => {
                let violations: Vec<_> = errors
                    .map(|e| json!({
                        "path": e.instance_path.to_string(),
                        "message": e.to_string(),
                    }))
                    .collect();
                
                PolicyDecision::Deny {
                    tool: tool.to_string(),
                    code: "E_ARG_SCHEMA".to_string(),
                    reason: "JSON Schema validation failed".to_string(),
                    contract: json!({
                        "status": "deny",
                        "error_code": "E_ARG_SCHEMA",
                        "tool": tool,
                        "violations": violations,
                    }),
                }
            }
        }
    }
}

/// Decision enum with warning variant
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
```

#### 6.4 Migration Logic

```rust
// crates/assay-core/src/mcp/migrate.rs

impl McpPolicy {
    /// Convert v1 regex constraints to v2 JSON Schema (in-place)
    pub fn migrate_constraints_to_schemas(&mut self) {
        for constraint in std::mem::take(&mut self.constraints) {
            let schema = constraint_to_schema(&constraint);
            self.schemas.insert(constraint.tool.clone(), schema);
        }
        self.version = "2.0".to_string();
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
                "minLength": 1,           // Security default
                "maxLength": 4096,        // Security default
            });
            required.push(param_name.clone());
        }
    }
    
    json!({
        "type": "object",
        "additionalProperties": false,    // Security default
        "properties": properties,
        "required": required,
    })
}

/// Export migrated policy to YAML string
pub fn export_v2_policy(policy: &McpPolicy) -> String {
    serde_yaml::to_string(policy).expect("Policy should serialize")
}
```

### Phase 2: CLI Integration (Week 2-3)

#### 6.5 New `assay policy` Subcommand Group

```rust
// crates/assay-cli/src/cli/args.rs

#[derive(Subcommand)]
pub enum Command {
    // ... existing commands
    
    /// Policy management commands
    Policy(PolicyArgs),
}

#[derive(Args)]
pub struct PolicyArgs {
    #[command(subcommand)]
    pub command: PolicyCommand,
}

#[derive(Subcommand)]
pub enum PolicyCommand {
    /// Migrate v1.x policy to v2.0 format
    Migrate(PolicyMigrateArgs),
    
    /// Validate policy syntax and schemas
    Validate(PolicyValidateArgs),
    
    /// Format policy file (normalize YAML)
    Fmt(PolicyFmtArgs),
}
```

#### 6.6 `assay policy migrate`

```rust
// crates/assay-cli/src/cli/commands/policy_migrate.rs

#[derive(Parser)]
pub struct PolicyMigrateArgs {
    /// Input policy file
    #[arg(short, long)]
    input: PathBuf,
    
    /// Output file (default: overwrite input)
    #[arg(short, long)]
    output: Option<PathBuf>,
    
    /// Preview changes without writing
    #[arg(long)]
    dry_run: bool,
}

pub async fn run(args: PolicyMigrateArgs) -> Result<i32> {
    let mut policy = McpPolicy::from_file(&args.input)?;
    
    if policy.version == "2.0" && policy.constraints.is_empty() {
        println!("✅ Policy is already v2.0 format");
        return Ok(0);
    }
    
    let constraint_count = policy.constraints.len();
    policy.migrate_constraints_to_schemas();
    
    let yaml = export_v2_policy(&policy);
    
    if args.dry_run {
        println!("--- Migration Preview ({} constraints → schemas) ---", constraint_count);
        println!("{}", yaml);
        return Ok(0);
    }
    
    let output_path = args.output.unwrap_or(args.input.clone());
    std::fs::write(&output_path, &yaml)?;
    
    println!("✅ Migrated {} constraints to v2.0 schemas: {}", 
             constraint_count, output_path.display());
    Ok(0)
}
```

#### 6.7 Update `assay coverage`

```rust
// crates/assay-cli/src/cli/commands/coverage.rs

pub async fn run_coverage(config: &CoverageConfig) -> Result<i32> {
    let policy = McpPolicy::from_file(&config.policy_path)?;
    // Warning emitted automatically if v1 constraints detected
    // Legacy shapes normalized automatically
    
    let mut state = PolicyState::default();
    
    for trace in &traces {
        for tool_call in &trace.tool_calls {
            let decision = policy.evaluate(&tool_call.name, &tool_call.arguments, &mut state);
            
            match decision {
                PolicyDecision::Allow => { /* pass */ }
                PolicyDecision::AllowWithWarning { code, reason, .. } => {
                    report.add_warning(&tool_call.name, &code, &reason);
                }
                PolicyDecision::Deny { code, contract, .. } => {
                    report.add_violation(&tool_call.name, &code, &contract);
                }
            }
        }
    }
    
    // ... rest of coverage output
}
```

### Phase 3: Server Simplification (Week 3-4)

#### 6.8 Server Uses Core Policy

```rust
// crates/assay-mcp-server/src/tools/check_args.rs

use assay_core::mcp::policy::{McpPolicy, PolicyDecision, PolicyState};

pub async fn handle_check_args(params: CheckArgsParams, policy_root: &Path) -> ToolResult {
    let policy_path = policy_root.join(&params.policy);
    let policy = McpPolicy::from_file(&policy_path)?;
    
    let mut state = PolicyState::default();
    let decision = policy.evaluate(&params.tool, &params.arguments, &mut state);
    
    match decision {
        PolicyDecision::Allow => {
            ToolResult::success(json!({"allowed": true}))
        }
        PolicyDecision::AllowWithWarning { code, reason, .. } => {
            ToolResult::success(json!({
                "allowed": true,
                "warning": { "code": code, "reason": reason }
            }))
        }
        PolicyDecision::Deny { code, reason, contract, .. } => {
            ToolResult::success(json!({
                "allowed": false,
                "code": code,
                "reason": reason,
                "violations": contract["violations"]
            }))
        }
    }
}
```

---

## 7. $ref Support (Scoped)

Support `$defs` for shared definitions within the same policy file:

```yaml
version: "2.0"
schemas:
  $defs:
    safe_path:
      type: string
      pattern: "^/workspace/.*"
      minLength: 1
      maxLength: 4096
  
  read_file:
    type: object
    additionalProperties: false
    properties:
      path: { $ref: "#/$defs/safe_path" }
    required: [path]
  
  list_directory:
    type: object
    additionalProperties: false
    properties:
      path: { $ref: "#/$defs/safe_path" }
    required: [path]
```

**Scope restriction:** Only `$ref` within the same document (`#/...`). No remote refs (supply chain risk).

**Implementation:** Schemas are compiled with access to the full policy document as the root, so refs like `#/$defs/...` resolve correctly. The `$defs` key is copied into each tool schema's compilation context.

---

## 8. Backward Compatibility

### Legacy Shape Normalization

v1 policies with root-level `allow`/`deny` or mixed nested shapes are automatically normalized into `tools.allow`/`tools.deny` on load:

```yaml
# v1 legacy (auto-normalized)
allow: [read_file]
deny: [write_file]

# Becomes internally:
tools:
  allow: [read_file]
  deny: [write_file]
```

### Constraint Migration

v1 `constraints` are auto-converted to `schemas` on load (with deprecation warning). Run `assay policy migrate` to persist the conversion.

---

## 9. Migration Guide

### CLI Commands

```bash
# Validate policy syntax
assay policy validate my-policy.yaml

# Preview migration
assay policy migrate --input my-policy.yaml --dry-run

# Apply migration
assay policy migrate --input my-policy.yaml

# Format policy (normalize YAML)
assay policy fmt my-policy.yaml
```

### Example: Before/After

**Before (v1.x):**
```yaml
version: "1.0"
allow: [read_file]  # Root-level (legacy)
constraints:
  - tool: read_file
    params:
      path:
        matches: "^/workspace/.*"
```

**After (v2.0):**
```yaml
version: "2.0"
tools:
  allow: [read_file]
enforcement:
  unconstrained_tools: warn
schemas:
  read_file:
    type: object
    additionalProperties: false
    properties:
      path:
        type: string
        pattern: "^/workspace/.*"
        minLength: 1
        maxLength: 4096
    required: [path]
```

---

## 10. Test Strategy

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_legacy_shape_normalization() {
        let yaml = r#"
allow: [read_file]
deny: [write_file]
"#;
        let policy: McpPolicy = serde_yaml::from_str(yaml).unwrap();
        let mut normalized = policy.clone();
        normalized.normalize_legacy_shapes();
        
        assert!(normalized.tools.allow.as_ref().unwrap().contains(&"read_file".to_string()));
        assert!(normalized.tools.deny.as_ref().unwrap().contains(&"write_file".to_string()));
    }
    
    #[test]
    fn test_v1_auto_migration() {
        let v1_yaml = r#"
version: "1.0"
constraints:
  - tool: read_file
    params:
      path:
        matches: "^/safe/.*"
"#;
        let mut policy: McpPolicy = serde_yaml::from_str(v1_yaml).unwrap();
        policy.migrate_constraints_to_schemas();
        
        assert_eq!(policy.version, "2.0");
        assert!(policy.schemas.contains_key("read_file"));
        assert!(policy.constraints.is_empty());
    }
    
    #[test]
    fn test_unconstrained_warn_mode() {
        let policy = McpPolicy {
            enforcement: EnforcementSettings {
                unconstrained_tools: UnconstrainedMode::Warn,
            },
            ..Default::default()
        };
        
        let mut state = PolicyState::default();
        let decision = policy.evaluate("unknown_tool", &json!({}), &mut state);
        
        assert!(matches!(decision, PolicyDecision::AllowWithWarning { .. }));
    }
    
    #[test]
    fn test_unconstrained_deny_mode() {
        let policy = McpPolicy {
            enforcement: EnforcementSettings {
                unconstrained_tools: UnconstrainedMode::Deny,
            },
            ..Default::default()
        };
        
        let mut state = PolicyState::default();
        let decision = policy.evaluate("unknown_tool", &json!({}), &mut state);
        
        assert!(matches!(decision, PolicyDecision::Deny { code, .. } if code == "E_TOOL_UNCONSTRAINED"));
    }
    
    #[test]
    fn test_migration_adds_security_defaults() {
        let constraint = ConstraintRule {
            tool: "read_file".to_string(),
            params: btreemap! {
                "path".to_string() => ConstraintParam { matches: Some("^/safe/.*".to_string()) }
            },
        };
        
        let schema = constraint_to_schema(&constraint);
        
        // Security defaults added
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["properties"]["path"]["minLength"], 1);
        assert_eq!(schema["properties"]["path"]["maxLength"], 4096);
    }
    
    #[test]
    fn test_wildcard_patterns() {
        let policy = McpPolicy {
            tools: ToolPolicy {
                deny: Some(vec!["execute_*".to_string(), "*_dangerous".to_string()]),
                ..Default::default()
            },
            ..Default::default()
        };
        
        assert!(policy.is_denied("execute_command"));
        assert!(policy.is_denied("execute_shell"));
        assert!(policy.is_denied("run_dangerous"));
        assert!(!policy.is_denied("read_file"));
    }
    
    #[test]
    fn test_reserved_keys_skipped() {
        let policy = McpPolicy {
            schemas: hashmap! {
                "$defs".to_string() => json!({"safe_path": {"type": "string"}}),
                "read_file".to_string() => json!({"type": "object"}),
            },
            ..Default::default()
        };
        
        let compiled = policy.compiled_schemas();
        
        // $defs should not be compiled as a tool schema
        assert!(!compiled.contains_key("$defs"));
        assert!(compiled.contains_key("read_file"));
    }
}
```

---

## 11. Files Changed

| File | Change |
|------|--------|
| `crates/assay-core/src/mcp/policy.rs` | Add `schemas`, `enforcement`, unified `evaluate()`, legacy normalization |
| `crates/assay-core/src/mcp/migrate.rs` | NEW: v1→v2 migration with security defaults |
| `crates/assay-core/src/policy_engine.rs` | Merge into `policy.rs`, keep JSON Schema compile logic |
| `crates/assay-cli/src/cli/args.rs` | Add `Policy` subcommand group |
| `crates/assay-cli/src/cli/commands/policy_migrate.rs` | NEW |
| `crates/assay-cli/src/cli/commands/policy_validate.rs` | NEW |
| `crates/assay-cli/src/cli/commands/coverage.rs` | Use unified `policy.evaluate()` |
| `crates/assay-mcp-server/src/tools/check_args.rs` | Import `McpPolicy` from core |
| `examples/policies/**/*.yaml` | Migrate to v2.0 |

---

## 12. Consequences

### Positive
| Benefit | Impact |
|---------|--------|
| Single policy format | Write once, use in CLI and Server |
| Industry standard | JSON Schema knowledge transfers |
| Security defaults | `additionalProperties: false`, length limits |
| Unconstrained warnings | Catch "forgot to add schema" errors |
| Extensible CLI | `assay policy` group for future commands |
| Legacy compat | Auto-normalization of v1 shapes |

### Negative
| Risk | Mitigation |
|------|------------|
| Breaking change | `assay policy migrate`, 2 minor versions deprecation |
| CLI namespace conflict | New `assay policy migrate` (not bare `assay migrate`) |
| Performance | All schemas compiled at first use, cached with `OnceLock<Arc<...>>` |

---

## 13. Resolved Questions

| Question | Decision | Rationale |
|----------|----------|-----------|
| `schemas` location | Top-level | Cleaner diffs, matches common JSON Schema practice |
| Tools without schema | `warn` default | Catch mistakes without breaking existing deployments |
| `$ref` support | Scoped to same doc | Prevent supply chain attacks via remote refs |
| `$ref` compilation | Full doc context | Compile with root document for ref resolution |
| CLI command name | `assay policy migrate` | Avoids conflict with existing `assay migrate` |
| Security defaults | Auto-added on migration | `additionalProperties: false`, `maxLength: 4096` |
| Wildcard scope | prefix/suffix/contains only | No glob-in-middle (`foo*bar`) support |
| Reserved keys | `$`-prefixed keys | Cannot be tool names, reserved for JSON Schema meta |
| Legacy shapes | Auto-normalized | Root-level allow/deny → tools.allow/deny |

---

## 14. References

- [JSON Schema Specification](https://json-schema.org/)
- [jsonschema crate](https://docs.rs/jsonschema/)
- `crates/assay-core/src/mcp/policy.rs`
- `crates/assay-core/src/policy_engine.rs`
