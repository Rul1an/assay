# MCP Tool Taxonomy + Coverage Runbook (v1)

## Intent
Operational guide for the MCP governance spine:
- tool taxonomy (classes)
- class-aware allow/deny evaluation
- coverage report generation (offline)
- coverage emission from `assay mcp wrap` (runtime)
- session/state window contract reference (ADR-029)

This runbook is informational and does not change enforcement behavior.

## What shipped on `main`
### Policy surface
- `tool_classes`: a taxonomy map of tool -> classes
- `tools.allow_classes` / `tools.deny_classes`: class-based matching in allow/deny evaluation
- decision events include deterministic `matched_tool_classes`

### Coverage surface
- `assay coverage --input <jsonl> --out <path> [--declared-tool ...]`
  - input accepts `tool` (preferred) or `tool_name` (fallback)
- `assay mcp wrap --coverage-out <path> ...`
  - emits `coverage_report_v1` based on decision log normalization
  - no core proxy changes required

### Contracts
- `schemas/coverage_report_v1.schema.json` (ADR-028)
- `schemas/session_state_window_v1.schema.json` (ADR-029, freeze only)

## Quickstart: classify tools (taxonomy)
1. Define classes per tool in policy.
   Prefer stable class strings such as:
   - `sink:network`
   - `source:local`
   - `source:sensitive`
   - `exec:system`
   - `store:disk`
2. Use allow/deny by class.
   - block all network sinks:
     - `tools.deny_classes: ["sink:network"]`
   - allow only local sources:
     - `tools.allow_classes: ["source:local"]`

## Quickstart: generate coverage offline
Given a JSONL file of tool events:

```bash
assay coverage \
  --input traces.ndjson \
  --out artifacts/coverage_report_v1.json \
  --declared-tool read_document \
  --declared-tool web_search
```

Interpretation:
- `tools.tools_unknown`: observed tools not declared by policy/config
- `taxonomy.tool_classes_missing`: observed tools with no taxonomy entry (blind spots)
- `routes.routes_seen`: adjacent tool-call edges (v1 is order-based, no session inference)

## Quickstart: emit coverage from an MCP wrapped session

```bash
assay mcp wrap \
  --policy policy.yaml \
  --event-source assay://local/demo \
  --coverage-out artifacts/coverage_report_v1.json \
  -- <mcp-host-cmd> <args...>
```

Notes:
- If no `--decision-log` is provided, a temp decision log is created and cleaned up.
- Coverage generation failures return exit code `2` (measurement/contract) but do not mask wrapped process failures.

## Operational interpretation guidelines
### Completeness questions this answers
- Did we see any tools that our policy did not explicitly declare?
- Are we missing taxonomy for tools that occurred in production traces?
- What tool-to-tool adjacency edges are actually exercised?

### What this does NOT claim
- No session/state inference in coverage v1 routes.
- No taint tracking or semantic flow labeling.
- No enforcement changes from coverage emission.

## Troubleshooting
### Coverage-out exists but is empty or missing fields
- Ensure decision events contain `data.tool` or top-level `tool|tool_name`.
- Ensure the wrapped command produced at least one decision event.

### Coverage generation fails (exit 2)
- Most common: decision log line missing tool identity (`tool`/`tool_name`).
- Verify input JSONL is line-delimited valid JSON.

## References
- ADR-028 Coverage Report
- ADR-029 Session/State Window Contract (freeze)
- Coverage schema: `schemas/coverage_report_v1.schema.json`
- Session/state schema: `schemas/session_state_window_v1.schema.json`
