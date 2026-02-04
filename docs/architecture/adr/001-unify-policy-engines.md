# ADR 001: Unify Policy Engines (JSON Schema vs Regex)

**Status:** Proposed in v1.5.1
**Date:** 2026-01-07
**Author:** Antigravity (on behalf of Roel Schuurkes)

## 1. Context

Assay currently maintains two divergent policy execution engines, leading to user confusion and tooling incompatibility.

### Engine A: The Core Engine (CLI)
*   **Location:** `crates/assay-core/src/mcp/policy.rs`
*   **Struct:** `McpPolicy`
*   **Logic:** Uses custom **Regex** constraints defined in a `constraints` array.
*   **Used By:** `assay coverage` command (`crates/assay-cli/src/cli/commands/coverage.rs`).
*   **Pros:** faster cold start (simple string matching).
*   **Cons:** Non-standard syntax, limited expressiveness (no numeric ranges, no array constraints).

### Engine B: The Server Engine (Runtime)
*   **Location:** `crates/assay-mcp-server/src/tools/check_args.rs`
*   **Struct:** Raw `serde_json::Value` (No struct enforcement).
*   **Logic:** Uses the **JSON Schema** standard via `jsonschema` crate.
*   **Used By:** `assay-mcp-server` binary / `assay_check_args` tool.
*   **Pros:** Industry standard, highly expressive, matches MCP spec.
*   **Cons:** Slightly heavier compilation cost.

### The Problem
A user cannot use the same policy file for both offline analysis (`assay coverage`) and runtime protection (`assay-mcp-server`).
*   `coverage` fails on JSON Schema syntax ("unknown field").
*   `server` fails on Core policy syntax ("E_POLICY_MISSING_TOOL" because it expects root keys to be tool names).

## 2. Decision

We will **standardize on JSON Schema** as the single source of truth for MCP policies in Assay.

### 2.1 The Unified Schema
The new `McpPolicy` struct in `assay-core` will act as a hybrid wrapper during the transition, but ultimately favor the Server's structure:

```yaml
# Unified Policy Format (v2.0)
version: "2.0"
tools:
  read_file:
    type: object
    properties:
      path: { type: string, pattern: "^/safe/.*" }
```

### 2.2 Implementation Plan

1.  **Refactor `assay-core`**:
    *   Update `McpPolicy` to deserialize tool constraints as `HashMap<String, serde_json::Value>` (representing JSON Schemas).
    *   Deprecate the `ConstraintRule` (regex) struct.
    *   Update `policy_engine::evaluate_tool_args` to prefer `jsonschema` compilation over regex matching.

2.  **Update `assay-cli`**:
    *   Update `coverage.rs` to use the new `policy_engine` logic.
    *   Add JSON Schema validation to the coverage analyzer.

3.  **Simplify `assay-mcp-server`**:
    *   Remove custom parsing logic in `check_args.rs`.
    *   Import and use the unified `McpPolicy` struct from `assay-core`.

## 3. Consequences

### Positive
*   **Single Truth:** One policy file validation for both CI checks and runtime.
*   **Standardization:** Users already know JSON Schema; no need to learn custom Assay regex syntax.
*   **Validation:** Can validate complex constraints (e.g. `minItems`, `exclusiveMaximum`) impossible with regex.

### Negative
*   **Breaking Change:** Existing v1.0 policies (using `constraints: [...]`) will require migration.
    *   *Mitigation:* Create a `assay migrate` command to auto-convert regex constraints to JSON Schema `pattern` properties.
*   **Performance:** `jsonschema::JSONSchema::compile` is heavier than regex compilation.
    *   *Mitigation:* Ensure `assay-mcp-server` caches compiled schemas (it already does, but `assay-core` needs to support this).

## 4. References

*   `crates/assay-core/src/mcp/policy.rs` (Current Core)
*   `crates/assay-mcp-server/src/tools/check_args.rs` (Current Server)
*   Issue #151 (Incompatible Policies)
