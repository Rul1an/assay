# Wave6 Step3 checklist: nightly safety lane

Scope lock:
- add non-blocking nightly workflow only
- docs + reviewer gates for Step3 only
- no production crate code changes

Artifacts:
- `docs/contributing/SPLIT-INVENTORY-wave6-step3-nightly.md`
- `docs/contributing/SPLIT-CHECKLIST-wave6-step3-nightly.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave6-step3-nightly.md`
- `scripts/ci/review-wave6-step3-ci.sh`
- `.github/workflows/wave6-nightly-safety.yml`

Runbook:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave6-step3-ci.sh
# stacked PRs: BASE_REF=origin/codex/wave6-step2-attestation-pair
```

Hard gates:
- workflow has `schedule` + `workflow_dispatch`.
- smoke jobs include `continue-on-error: true`.
- miri and property smoke anchor commands present.
- strict diff allowlist for Step3 files.

Definition of done:
- reviewer script passes.
- nightly lane is non-blocking and does not change required PR checks.
