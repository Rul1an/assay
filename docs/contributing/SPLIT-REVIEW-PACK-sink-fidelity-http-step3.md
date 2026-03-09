# Sink-fidelity HTTP Step3 Review Pack (Closure)

## Intent

Close Wave22 with a docs+gate-only slice after Step2 bounded implementation merged on `main`.

Step3 must not modify implementation files.

## Allowed scope

- `docs/contributing/SPLIT-CHECKLIST-sink-fidelity-http-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-sink-fidelity-http-step3.md`
- `scripts/ci/review-exp-mcp-fragmented-ipi-sink-fidelity-http-step3.sh`

## Not allowed

- `.github/workflows/*`
- `scripts/ci/exp-mcp-fragmented-ipi/**`
- `scripts/ci/test-exp-mcp-fragmented-ipi-sink-failure.sh`
- any additional docs outside Step3 closure artifacts

## Revalidated Step2 contract

- run shape remains frozen (`RUNS_ATTACK=2`, `RUNS_LEGIT=100`)
- fidelity marker remains frozen (`SINK_FIDELITY_MODE=http_local`)
- primary metric remains attempt-based (`success_any_sink_canary`)
- completion-layer fields remain present

## Acceptance re-check

- `wrap_only` remains inferior where expected
- `sequence_only` and `combined` remain robust on protected attack path
- `combined == sequence_only` on protected outcomes
- protected legit false-positive rate remains `0.0`

## Reviewer command

```bash
BASE_REF=origin/main bash scripts/ci/review-exp-mcp-fragmented-ipi-sink-fidelity-http-step3.sh
```
