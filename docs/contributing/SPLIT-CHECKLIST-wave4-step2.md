# Wave4 Step2 checklist (lockfile/cache mechanical split)

Scope lock:
- Step2 is a mechanical split behind stable facades.
- No behavior/perf changes intended.
- Public module paths remain unchanged.

Artifacts:
- `docs/contributing/SPLIT-MOVE-MAP-wave4-step2.md`
- `docs/contributing/SPLIT-CHECKLIST-wave4-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave4-step2.md`
- `scripts/ci/review-wave4-step2.sh`

Critical boundaries:
- Stable ordering logic is single-source in `lockfile_next/format.rs`.
- Atomic write/rename logic is single-source in `cache_next/io.rs`.
- Facades delegate to `*_next` impl paths for moved logic.

Reviewer command:
```bash
BASE_REF=origin/codex/wave3-step1-behavior-freeze-v2 bash scripts/ci/review-wave4-step2.sh
```

Definition of done:
- fmt/clippy/check pass for `assay-registry`.
- Lockfile/cache contract-anchor tests pass.
- Delegation and single-source gates pass.
- Diff allowlist contains only Step2 scope files.
