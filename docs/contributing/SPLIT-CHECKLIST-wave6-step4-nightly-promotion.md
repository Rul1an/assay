# Wave6 Step4 checklist: nightly promotion policy

Scope lock (Commit A):
- docs + reviewer gates only
- no workflow semantic changes
- no required-check/branch-protection changes

Artifacts:
- `docs/contributing/SPLIT-INVENTORY-wave6-step4-nightly-promotion.md`
- `docs/contributing/SPLIT-CHECKLIST-wave6-step4-nightly-promotion.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave6-step4-nightly-promotion.md`
- `scripts/ci/review-wave6-step4-ci.sh`
- `docs/architecture/PLAN-split-refactor-2026q1.md`

Runbook:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave6-step4-ci.sh
```

Metric criteria (policy freeze for Step4):

| Metric | Window | Threshold | How measured | Category rule |
|---|---|---|---|---|
| `scheduled_runs_count` | most recent 20 completed `schedule` runs on `main`; if more exist, use newest 20 | `>= 14` runs; if `< 14`, promotion-ready = false | GitHub Actions runs API (`workflow=wave6-nightly-safety.yml`, `event=schedule`) | n/a |
| `window_span_days` | same selected runs | `>= 14` days; if `< 14`, promotion-ready = false | `newest.created_at - oldest.created_at` in selected window | n/a |
| `smoke_success_rate_excl_infra` | same selected runs | `>= 95%` | `success_runs / (success_runs + test_fail_runs + flake_runs)` | `infra` excluded from denominator |
| `infra_failure_rate` | same selected runs | `<= 10%` | `infra_fail_runs / total_runs` | `infra` includes cancelled/timed_out/runner unavailable/network transient |
| `flake_rate` | same selected runs | `<= 5%` | `flake_runs / total_runs` | **Option 1 fixed**: flake only via `run_attempt > 1` (no cross-run SHA heuristic in Step4) |
| `duration_median_minutes` | same selected runs | `<= 20` | median of `workflow_duration_seconds / 60` | n/a |
| `duration_p95_minutes` | same selected runs | `<= 35` | p95 of `workflow_duration_seconds / 60` | n/a |
| `recent_red_streak` | last 10 selected runs | `<= 1` consecutive red | run sequence check | red = `test` or `flake`; `infra` tracked separately |
| `required_check_impact` | policy invariant | `0` required-check changes | repo policy: Step4 does not edit branch protection or required check config | n/a |

Classification notes (Step4):
- `success`: smoke command exits `0`.
- `infra`: cancellation/timeout/runner/network/transient infra signature.
- `test`: deterministic assertion/test failure.
- `flake`: retry-attempt signal only (`run_attempt > 1`) with eventual success.

Commit B requirement (schema freeze now, implementation later):
- nightly workflow uploads `nightly_status.json` artifact.
- schema must include:
  - `schema_version` (integer)
  - `classifier_version` (integer)
  - `repo`, `workflow_file`, `run_id`, `run_attempt`, `sha`
  - per-job `job_id`, `conclusion`, `outcome`, `category`, `duration_seconds`
  - workflow-level `workflow_conclusion`, `workflow_category`, aggregate counters

Hard gates (Commit A script):
- nightly workflow still has `schedule` + `workflow_dispatch`.
- smoke jobs remain `continue-on-error: true`.
- permissions remain minimal (`permissions: {}` and no `id-token: write` in nightly lane).
- baseline currently has no workflow `concurrency` or `timeout-minutes`; Commit A keeps this unchanged.
- strict diff allowlist for Step4 Commit A files.

Definition of done (Commit A):
- reviewer script passes against `origin/main`.
- policy criteria and formulas are frozen and audit-ready.
- no workflow behavior changes landed in Commit A.
