# Interleaving Step3 Review Pack (Closure)

## Intent

Close Wave23 with a docs+gate-only closure slice after Step2 bounded interleaving implementation.

## Scope

- `docs/contributing/SPLIT-CHECKLIST-interleaving-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-interleaving-step3.md`
- `scripts/ci/review-exp-mcp-fragmented-ipi-interleaving-step3.sh`

## Non-goals

- no harness/scorer code changes
- no workflow changes
- no scope expansion outside this closure package

## Closure validation contract

Step3 re-runs Step2 invariants:

- frozen run shape remains: `RUNS_ATTACK=2`, `RUNS_LEGIT=100`
- attempt-based metric remains: `success_any_sink_canary`
- interleaving fields remain published:
  - `case_id`, `ordering_class`, `segment_label`, `turn_index`
  - pre-attempt / attempt markers
- bounded interleaving smoke passes
- acceptance remains unchanged:
  - `sequence_only` and `combined` keep robust protected outcomes
  - `wrap_only` remains inferior where expected
  - `combined` equals `sequence_only` on protected outcomes

## Reviewer commands

Against stacked Step2 base:

```bash
BASE_REF=origin/codex/wave23-interleaving-step2-impl \
  bash scripts/ci/review-exp-mcp-fragmented-ipi-interleaving-step3.sh
```

Against origin/main after sync:

```bash
BASE_REF=origin/main \
  bash scripts/ci/review-exp-mcp-fragmented-ipi-interleaving-step3.sh
```

## Reviewer 60s scan

1. Diff contains only Step3 checklist/review-pack/script.
2. Workflow-ban is present.
3. Step3 script re-runs run-shape markers + field markers + bounded smoke + acceptance checks.
4. Script passes against stacked base and `origin/main`.
