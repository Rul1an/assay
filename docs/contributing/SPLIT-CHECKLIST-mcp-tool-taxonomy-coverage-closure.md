# SPLIT CHECKLIST — MCP Tool Taxonomy + Coverage Closure (C-slice)

## Scope discipline
- [ ] Only docs + reviewer gate changed
- [ ] No `.github/workflows/*` changes
- [ ] No Rust code changes
- [ ] No schema changes

## Runbook completeness
- [ ] Explains taxonomy + allow/deny classes
- [ ] Explains offline `assay coverage` usage
- [ ] Explains `assay mcp wrap --coverage-out` usage
- [ ] Lists bounded non-goals (no session inference, no taint tracking)
- [ ] Troubleshooting section present

## Consistency checks (manual spot-check)
- [ ] Mentions ADR-028 / ADR-029 correctly
- [ ] Uses correct schema names:
  - [ ] `coverage_report_v1`
  - [ ] `session_state_window_v1`
- [ ] Uses correct CLI flags:
  - [ ] `assay coverage --input/--out/--declared-tool`
  - [ ] `assay mcp wrap --coverage-out`

## Gate
- [ ] Reviewer gate enforces allowlist-only
- [ ] Reviewer gate bans workflows
- [ ] Reviewer gate marker checks pass
