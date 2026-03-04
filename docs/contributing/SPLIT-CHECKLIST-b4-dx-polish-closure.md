# SPLIT CHECKLIST — B4 DX Polish Closure (C-slice)

## Scope discipline
- [ ] Only docs + reviewer gate changed
- [ ] No `.github/workflows/*` changes
- [ ] No Rust code changes
- [ ] No schema/ADR changes

## Runbook completeness
- [ ] Documents `assay coverage --format md`
- [ ] Documents `assay coverage --declared-tools-file`
- [ ] Documents `assay mcp wrap --coverage-out`
- [ ] Documents `assay mcp wrap --state-window-out`
- [ ] Documents invariant `wrapped > coverage > state-window`
- [ ] Includes bounded non-goals

## Gate
- [ ] Reviewer gate allowlist-only
- [ ] Reviewer gate workflow-ban
- [ ] Marker checks pass
