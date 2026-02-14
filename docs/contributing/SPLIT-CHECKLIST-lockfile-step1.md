# Wave4 Step1 checklist: lockfile.rs

Scope lock:
- Step1 is behavior freeze only.
- No module split in this step.
- No behavior/perf changes.
- No dependency/Cargo changes in this step.

Artifacts:
- `docs/contributing/SPLIT-INVENTORY-wave4-step1.md`
- `docs/contributing/SPLIT-SYMBOLS-wave4-step1.md`
- `scripts/ci/review-wave4-step1.sh`

Contract anchors:
- `test_lockfile_v2_roundtrip`
- `test_lockfile_stable_ordering`
- `test_lockfile_digest_mismatch_detection`
- `test_lockfile_signature_fields`

Drift gates (best-effort code-only):
- no increase in `unwrap/expect`
- no increase in `unsafe`
- no increase in `println/eprintln`
- no increase in `dbg/trace/debug`
- no increase in `panic/todo/unimplemented`

Runbook:
```bash
BASE_REF=origin/codex/wave3-step1-behavior-freeze-v2 bash scripts/ci/review-wave4-step1.sh
```

Definition of done:
- Freeze test subset green.
- Drift counters unchanged.
- Diff stays within allowlist.
