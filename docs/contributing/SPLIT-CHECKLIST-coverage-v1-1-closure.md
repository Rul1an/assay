# SPLIT CHECKLIST — Coverage v1.1 Closure (C-slice)

## Scope discipline
- [ ] Only docs + reviewer gate changed
- [ ] No `.github/workflows/*` changes
- [ ] No Rust code changes
- [ ] No schema / ADR changes

## Runbook correctness
- [ ] Documents `--out-md`
- [ ] Documents `--routes-top`
- [ ] States JSON (`coverage_report_v1`) is canonical
- [ ] States markdown is derived output
- [ ] Documents declared-tools-file semantics (1 per line, comments, union)
- [ ] Documents exit codes (0/2/3) for generator mode
- [ ] Bounded non-goals present (no workflows, no MCP wrap change, no schema bump)

## Reviewer gate
- [ ] Allowlist-only
- [ ] Workflow-ban
- [ ] Marker checks for critical content
