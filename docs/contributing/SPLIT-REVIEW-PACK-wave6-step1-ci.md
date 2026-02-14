# Review Pack: Wave6 Step1 (CI/CD hardening freeze)

Intent:
- establish a reviewable CI/CD baseline before changing attestation/nightly lanes.

Scope:
- docs + reviewer script only.
- no workflow semantic changes.

Reviewer script:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave6-step1-ci.sh
```

What the script proves:
- current baseline anchors still exist in workflows.
- Step1 diff stays inside an explicit allowlist.
- BASE_REF resolution is explicit and logged.

Follow-up target (Wave6 Step2+):
- add attestation producer + verification pair (fail closed).
- add nightly fuzz/model lane (non-blocking first).
- keep existing Wave0 gates intact while introducing new checks.
