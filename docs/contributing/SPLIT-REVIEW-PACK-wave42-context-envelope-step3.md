# SPLIT REVIEW PACK - Wave42 Context Envelope Step3

## Intent
Close Wave42 with a docs+gate-only closure slice after bounded context-envelope hardening implementation in Step2.

This slice must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add new runtime capability
- expand policy-engine/control-plane/auth transport scope
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-CHECKLIST-wave42-context-envelope-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave42-context-envelope-step3.md`
- `scripts/ci/review-wave42-context-envelope-step3.sh`

## What reviewers should verify
1. Diff is docs+script only.
2. Step3 reruns the same structural invariants from Step2.
3. Wave42 context-envelope markers remain present.
4. Deterministic context completeness markers remain present.
5. Existing replay/decision markers remain present.
6. No runtime behavior scope creep appears in closure slice.
7. Pinned tests still pass.

## Reviewer commands

### Against stacked Step2 base
```bash
BASE_REF=origin/codex/wave42-context-envelope-step2-impl \
  bash scripts/ci/review-wave42-context-envelope-step3.sh
```
Use this only when the stacked Step2 ref is synced with current `main` history.

### Against origin/main after sync
```bash
BASE_REF=origin/main \
  bash scripts/ci/review-wave42-context-envelope-step3.sh
```

Expected outcome:
- Step3 adds no runtime behavior.
- closure remains diff-proof.
- promote can happen cleanly after authoritative `origin/main` validation.
