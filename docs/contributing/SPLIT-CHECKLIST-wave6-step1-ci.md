# Wave6 Step1 checklist: CI/CD hardening freeze

Scope lock:
- Step1 is docs + reviewer gates only.
- No workflow semantics change in this step.
- No production crate code changes in this step.

Artifacts:
- `docs/contributing/SPLIT-INVENTORY-wave6-step1-ci.md`
- `docs/contributing/SPLIT-CHECKLIST-wave6-step1-ci.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave6-step1-ci.md`
- `scripts/ci/review-wave6-step1-ci.sh`

Runbook:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave6-step1-ci.sh
```

Hard gates (script-enforced):
- BASE_REF resolve guard + effective SHA print.
- Baseline workflow anchors still present (feature matrix/nextest/semver/clippy anti-placeholder, attestation conditional, id-token write).
- Strict diff allowlist fail-fast for Step1 docs/script/plan scope.

Definition of done:
- reviewer script passes.
- no workflow behavior changes in this PR.
