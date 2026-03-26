# SPLIT REVIEW PACK - Wave43 Decision Kernel Step1

## Intent
Freeze a bounded split plan for the MCP decision kernel before any Step2 implementation work.

This slice is docs + gate only.

It must not:
- change MCP runtime behavior
- change event payload shape
- rename reason codes
- change replay/contract refresh behavior
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-PLAN-wave43-decision-kernel.md`
- `docs/contributing/SPLIT-CHECKLIST-wave43-decision-kernel-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave43-decision-kernel-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave43-decision-kernel-step1.md`
- `scripts/ci/review-wave43-decision-kernel-step1.sh`

## What reviewers should verify
1. Diff is limited to the five Step1 files.
2. Stable public decision/event surfaces are explicit.
3. Proposed `decision_next/` boundaries are explicit and mechanical.
4. Event payload and reason-code freezes are explicit.
5. Replay/contract refresh freeze is explicit.
6. Runtime code and tests are untouched in this step.
7. Scope does not expand into handler, policy, CLI, or MCP server work.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave43-decision-kernel-step1.sh
```

Expected outcome:
- gate passes
- runtime code is untouched
- the future Step2 move set is explicit
- Step2 can proceed mechanically without reopening contract scope
