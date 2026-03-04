# Coverage and Wrap DX Runbook (B4)

## Intent
Operational guidance for B4 DX polish on main:
- `assay coverage --format md`
- `assay coverage --declared-tools-file <path>`
- consistent wrap export logging for `--coverage-out` and `--state-window-out`

This runbook is documentation-only and does not change enforcement behavior.

## What Shipped
- Coverage input mode accepts declared tools from:
  - repeated `--declared-tool`
  - `--declared-tools-file` (one tool per line, `#` comments ignored)
- Coverage output format in input mode supports markdown summary via `--format md`.
- Wrap export logging now uses one consistent line per successful write:
  - `Wrote coverage_report_v1 to <path>`
  - `Wrote session_state_window_v1 to <path>`

## Quickstart

### Coverage JSON output
```bash
assay coverage \
  --input traces.ndjson \
  --out artifacts/coverage_report_v1.json \
  --declared-tool read_document \
  --declared-tool web_search
```

### Coverage markdown output
```bash
assay coverage \
  --input traces.ndjson \
  --out artifacts/coverage_report_v1.md \
  --declared-tools-file scripts/ci/fixtures/coverage/declared_tools_basic.txt \
  --format md
```

### Wrap with both exports
```bash
assay mcp wrap \
  --policy assay.yaml \
  --event-source assay://local/demo \
  --coverage-out artifacts/coverage_report_v1.json \
  --state-window-out artifacts/session_state_window_v1.json \
  -- <mcp-host-cmd> <args...>
```

Direct examples for reviewers:
- `assay mcp wrap --coverage-out artifacts/coverage_report_v1.json -- <mcp-host-cmd> <args...>`
- `assay mcp wrap --state-window-out artifacts/session_state_window_v1.json -- <mcp-host-cmd> <args...>`

## Exit Priority Invariant
Wrapped process exit remains authoritative.

The invariant is unchanged and must remain:
- `wrapped > coverage > state-window`

Meaning:
- If wrapped fails, its exit code is returned.
- If wrapped succeeds but coverage generation fails, coverage exit is returned.
- If wrapped and coverage succeed but state window export fails, state-window exit is returned.

## Error Mapping
- input/contract parsing failures: exit `2`
- output/write failures: exit `3`

## Bounded Non-Goals
- No policy DSL changes.
- No schema changes (`coverage_report_v1`, `session_state_window_v1` unchanged).
- No workflow/required-check changes.
- No runtime enforcement behavior changes.

## References
- ADR-030 Coverage + Wrap DX Polish
- `/Users/roelschuurkes/assay/schemas/coverage_report_v1.schema.json`
- `/Users/roelschuurkes/assay/schemas/session_state_window_v1.schema.json`
