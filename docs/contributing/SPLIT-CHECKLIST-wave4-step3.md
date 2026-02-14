# Wave4 Step3 checklist (`explain.rs` split)

Scope lock:
- Mechanical split behind stable facade.
- No behavior/perf changes intended.
- Public symbols remain under `crate::explain::*`.

Artifacts:
- `docs/contributing/SPLIT-MOVE-MAP-wave4-step3.md`
- `docs/contributing/SPLIT-CHECKLIST-wave4-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave4-step3.md`
- `scripts/ci/review-wave4-step3.sh`

Critical boundaries:
- Facade only in `explain.rs` (no state machine/render internals).
- Rule/state machine single-source in `explain_next/diff.rs`.
- Output formatting single-source in `explain_next/render.rs`.

Reviewer command:
```bash
BASE_REF=origin/codex/wave4-step2x-cache-thinness bash scripts/ci/review-wave4-step3.sh
```

Definition of done:
- fmt/clippy/check pass for `assay-core`.
- explain contract anchors pass.
- facade/single-source gates pass.
- diff allowlist contains only Step3 scope files.
