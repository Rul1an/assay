# SPLIT REVIEW PACK — MCP Tool Taxonomy + Coverage Closure (C-slice)

## Intent
Close the MCP governance spine line with operational documentation and a reviewer gate:
- tool taxonomy + class-aware allow/deny
- coverage report generation + wrap emission
- bounded claims and non-goals

## Scope
Docs + reviewer script only:
- `docs/ops/MCP-TOOL-TAXONOMY-AND-COVERAGE-RUNBOOK.md`
- `docs/contributing/SPLIT-CHECKLIST-mcp-tool-taxonomy-coverage-closure.md`
- `docs/contributing/SPLIT-REVIEW-PACK-mcp-tool-taxonomy-coverage-closure.md`
- `scripts/ci/review-mcp-tool-taxonomy-coverage-closure.sh`

## Safety contracts
- No runtime changes
- No workflow changes
- No schema changes

## Reviewer quick-check (60s)
1. Open the runbook and ensure it includes:
   - `assay coverage --input` and `assay mcp wrap --coverage-out`
   - bounded non-goals
2. Run reviewer gate:

```bash
BASE_REF=origin/main bash scripts/ci/review-mcp-tool-taxonomy-coverage-closure.sh
```

## Expected outcome
- Gate passes
- Documentation is sufficient for a new developer to use taxonomy + coverage without reading code
