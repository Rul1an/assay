# Review Pack: Wave6 Step3 (nightly safety lane)

Intent:
- add a non-blocking nightly/model safety lane with concrete smoke anchors.

Scope:
- `.github/workflows/wave6-nightly-safety.yml`
- docs/reviewer artifacts for Step3

Nightly jobs:
- `miri-registry-smoke` (continue-on-error)
- `proptest-cli-smoke` (continue-on-error)
- `nightly-summary` (always)

Reviewer command:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave6-step3-ci.sh
# stacked PRs: BASE_REF=origin/codex/wave6-step2-attestation-pair
```

Expected result:
- PASS with allowlisted diff only.
