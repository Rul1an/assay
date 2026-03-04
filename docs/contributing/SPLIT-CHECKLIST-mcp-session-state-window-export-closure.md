# SPLIT CHECKLIST — MCP Session/State Window Export Closure (C-slice)

## Scope discipline
- [ ] Only docs + reviewer gate changed
- [ ] No `.github/workflows/*` changes
- [ ] No Rust code changes
- [ ] No schema/ADR changes

## Runbook completeness
- [ ] Documents `assay mcp wrap --state-window-out`
- [ ] Documents exit priority: wrapped > coverage > state-window
- [ ] States deterministic snapshot id mechanism
- [ ] States privacy defaults (`stores_raw_* = false`)
- [ ] Bounded non-goals present (no backend, no enforcement, no cross-session decay)

## Consistency checks
- [ ] References ADR-029
- [ ] References `session_state_window_v1` schema name correctly
- [ ] Mentions `window_kind: session` for current export

## Gate
- [ ] Reviewer gate allowlist-only
- [ ] Reviewer gate bans workflows
- [ ] Marker checks pass
