# Review Pack: Wave6 Step4 (nightly promotion policy)

Intent:
- freeze a measurable, source-verifiable promotion policy for the nightly safety lane.

Scope:
- docs + reviewer script only (Commit A)
- no workflow edits in this commit

Reviewer command:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave6-step4-ci.sh
```

Expected result:
- PASS with allowlisted diff only.

Policy highlights:
- fixed window selection: newest 20 completed scheduled runs on `main`.
- promotion-ready is auto-false when fewer than 14 runs or fewer than 14 days of span.
- flake detection is deterministic in Step4: retry-attempt signal only (`run_attempt > 1`).
- required-check impact is policy-locked: Step4 does not change required checks/branch protection.

Commit B schema requirement (preview):
- workflow emits `nightly_status.json` artifact with `schema_version` + `classifier_version`.
- include raw and normalized outcomes (`conclusion` + `category`) and timing fields.
