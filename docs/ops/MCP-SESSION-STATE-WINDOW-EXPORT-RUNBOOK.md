# MCP Session/State Window Export Runbook (v1)

## Intent
Operational guide for the informational `session_state_window_v1` export from MCP wrap:

- CLI surface: `assay mcp wrap --state-window-out <path>`
- Contract: ADR-029 + `schemas/session_state_window_v1.schema.json`
- Guarantees:
  - schema-validated output
  - deterministic snapshot ids (canonical JSON -> sha256)
  - no backend / no enforcement (informational only)
  - wrapped process exit remains authoritative

## What shipped on `main`
- `assay mcp wrap --state-window-out <path>` writes a `session_state_window_v1` JSON file after the wrapped session completes.
- The report includes:
  - `session` tuple (`event_source`, `server_id`, `session_id`)
  - `window` (`window_kind: "session"` in v1 export)
  - `snapshot.state_snapshot_id` (deterministic content-addressed id)
  - `privacy` defaults (`stores_raw_* = false`)
- Output is validated against the embedded schema before writing.

## Quickstart
```bash
assay mcp wrap \
  --policy policy.yaml \
  --event-source assay://local/demo \
  --state-window-out artifacts/session_state_window_v1.json \
  -- <mcp-host-cmd> <args...>
```

## Exit semantics
- Wrapped command exit code is always authoritative:
  - if the wrapped command fails, its exit code is returned
- If wrapped succeeds:
  - coverage-out (if enabled) may still fail the overall run
  - state-window-out failures return:
    - `2` for measurement/contract issues (rare)
    - `3` for infra write issues
- Current priority is: wrapped > coverage > state-window.

## Interpretation

### What this report is good for
- Auditable session identity for MCP governance workflows.
- Stable, content-addressed snapshot ids for later linking/correlation.
- A minimal session-scope state window boundary for experiments and future features.

### What this does NOT claim (bounded)
- No cross-session decay logic is implemented by this export.
- No backend persistence is defined.
- No enforcement behavior is introduced.
- No taint tracking or semantic flow labeling.

## Privacy defaults
The export is intentionally safe-by-default:
- `stores_raw_tool_args = false`
- `stores_raw_prompt_bodies = false`
- `stores_raw_document_bodies = false`

Only identifiers and stable metadata are included.

## Troubleshooting

### Output file missing
- Ensure `--state-window-out` is set and points to a writable location.
- Check wrapped process exit code: failures may stop later exports depending on exit priority.

### Schema validation failure
- Indicates a bug/regression. Capture:
  - `assay --version`
  - the command line used
  - stderr output

## References
- ADR-029 Session & State Window Contract
- Schema: `schemas/session_state_window_v1.schema.json`
