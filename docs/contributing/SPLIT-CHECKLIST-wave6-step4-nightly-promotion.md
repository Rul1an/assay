# Wave6 Step4 checklist: nightly promotion policy + metrics

Artifacts:
- `docs/contributing/SPLIT-INVENTORY-wave6-step4-nightly-promotion.md`
- `docs/contributing/SPLIT-CHECKLIST-wave6-step4-nightly-promotion.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave6-step4-nightly-promotion.md`
- `scripts/ci/review-wave6-step4-ci.sh`
- `.github/workflows/wave6-nightly-safety.yml`
- `docs/architecture/PLAN-split-refactor-2026q1.md`

Runbook:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave6-step4-ci.sh
```

Scope lock:
- no production crate code changes
- no required-check/branch-protection changes

Metric criteria (frozen in Step4):

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

Commit B implementation choice:
- **Option A**: centralized generator in `nightly-summary` job fetches job data via GitHub Actions API and writes one JSON artifact.

Conclusion -> category mapping (Commit B contract):

| Raw conclusion | Category |
|---|---|
| `success` and `run_attempt == 1` | `success` |
| `success` and `run_attempt > 1` | `flake` |
| `failure` | `test` |
| `cancelled` or `timed_out` | `infra` |
| any other conclusion | `infra` |

Artifact contract (Commit B):
- artifact name: `nightly-status`
- file path: `nightly_status.json`
- retention: `14` days
- one artifact per workflow run (generated in `nightly-summary` only)

`nightly_status.json` schema contract:
- top-level:
  - `schema_version` (integer)
  - `classifier_version` (integer)
  - `repo`, `workflow_file`, `run_id`, `run_attempt`, `sha`
  - `workflow_conclusion`, `workflow_category`
- jobs:
  - `job_id`, `name`, `conclusion`, `outcome`, `category`, `duration_seconds`
- summary:
  - aggregate counters and `workflow_duration_seconds`

Hard gates (Step4 reviewer script):
- nightly workflow still has `schedule` + `workflow_dispatch`.
- smoke jobs remain `continue-on-error: true`.
 - `nightly-summary` permissions include only `actions: read` + `contents: read`.
- no `id-token: write` in nightly lane.
- artifact upload for `nightly_status.json` is present.
- strict diff allowlist for Step4 files.

Definition of done (Commit B):
- reviewer script passes against `origin/main`.
- metrics artifact is emitted with stable schema.
- nightly lane remains non-blocking and does not become required.
