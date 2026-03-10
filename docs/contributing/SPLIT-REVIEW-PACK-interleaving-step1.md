# SPLIT REVIEW PACK — Wave23 Interleaving Step1

## Intent
Freeze the next bounded hypothesis slice:
- mixed legit + malicious interleaving inside the fragmented-IPI harness family

Step1 is docs+gate only and must not mutate harness/runtime behavior.

## Allowed files
- `docs/contributing/SPLIT-PLAN-wave23-interleaving.md`
- `docs/contributing/SPLIT-CHECKLIST-interleaving-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-interleaving-step1.md`
- `scripts/ci/review-exp-mcp-fragmented-ipi-interleaving-step1.sh`

## Not allowed
- `.github/workflows/*`
- `scripts/ci/exp-mcp-fragmented-ipi/**`
- `scripts/ci/test-exp-mcp-fragmented-ipi*.sh`
- scorer/harness/runtime code drift

## Reviewer checks
1. Diff is Step1 docs+gate only.
2. Boundaries and non-goals are explicit in the plan.
3. Gate enforces no-touch on experiment harness paths.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-exp-mcp-fragmented-ipi-interleaving-step1.sh
```
