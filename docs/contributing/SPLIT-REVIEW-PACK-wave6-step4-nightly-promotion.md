# Review Pack: Wave6 Step4 (nightly promotion policy)

Intent:
- implement Step4 metrics instrumentation behind a frozen promotion policy.

Scope:
- `.github/workflows/wave6-nightly-safety.yml`
- docs + reviewer script updates for Step4

Reviewer command:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave6-step4-ci.sh
```

Expected result:
- PASS with allowlisted diff only.

Policy highlights (frozen):
- fixed window selection: newest 20 completed scheduled runs on `main`.
- promotion-ready is auto-false when fewer than 14 runs or fewer than 14 days of span.
- flake detection is deterministic in Step4: retry-attempt signal only (`run_attempt > 1`).
- required-check impact is policy-locked: Step4 does not change required checks/branch protection.

Commit B implementation details:
- Option A aggregator in `nightly-summary` job (GitHub Actions API).
- One artifact per run:
  - artifact: `nightly-status`
  - file: `nightly_status.json`
  - retention: 14 days
- Schema includes:
  - `schema_version`, `classifier_version`
  - run metadata and workflow-level normalized fields
  - per-job `job_id` + raw `conclusion` + normalized `category`
- job permissions for aggregator: `actions: read`, `contents: read` only.
- nightly workflow remains non-blocking (`continue-on-error: true` on smoke jobs).

Commit C implementation details:
- add `wave6-nightly-readiness.yml` as a separate **informational** workflow.
- no `pull_request` trigger (prevents accidental required-check coupling).
- readiness report script computes promotion metrics from GitHub Actions API:
  - `scripts/ci/wave6-nightly-readiness-report.sh`
- readiness artifact:
  - name: `nightly-readiness-report`
  - files: `nightly_readiness_report.json`, `nightly_readiness_report.md`
  - retention: 14 days
- required-check policy remains unchanged.

Conclusion-to-category mapping:

| Raw conclusion | Category |
|---|---|
| `success` and `run_attempt == 1` | `success` |
| `success` and `run_attempt > 1` | `flake` |
| `failure` | `test` |
| `cancelled` or `timed_out` | `infra` |
| other values | `infra` |

Non-blocking proof snippet:
```bash
rg -n "continue-on-error:\\s*true" .github/workflows/wave6-nightly-safety.yml
```
