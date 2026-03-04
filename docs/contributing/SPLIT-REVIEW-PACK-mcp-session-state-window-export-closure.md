# SPLIT REVIEW PACK — MCP Session/State Window Export Closure (C-slice)

## Intent
Close the session/state export line with operational documentation and a reviewer gate.

## Scope
Docs + reviewer script only:
- `docs/ops/MCP-SESSION-STATE-WINDOW-EXPORT-RUNBOOK.md`
- `docs/contributing/SPLIT-CHECKLIST-mcp-session-state-window-export-closure.md`
- `docs/contributing/SPLIT-REVIEW-PACK-mcp-session-state-window-export-closure.md`
- `scripts/ci/review-mcp-session-state-window-export-closure.sh`

## Safety contracts
- No runtime changes
- No workflow changes
- No schema/ADR changes

## Reviewer quick-check (60s)
1. Confirm runbook includes:
- `assay mcp wrap --state-window-out`
- deterministic snapshot id and privacy defaults
- bounded non-goals
2. Run the reviewer gate:
```bash
BASE_REF=origin/main bash scripts/ci/review-mcp-session-state-window-export-closure.sh
```

## Expected outcome
- Gate passes.
- Documentation is sufficient for a new developer to use the export correctly.
