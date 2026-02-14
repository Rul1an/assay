# Wave4 Step1 checklist: cache.rs

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
- `test_cache_roundtrip`
- `test_cache_integrity_failure`
- `test_signature_json_corrupt_handling`
- `test_atomic_write_prevents_partial_cache`

Drift gates (best-effort code-only):
- no increase in `unwrap/expect`
- no increase in `unsafe`
- no increase in `println/eprintln`
- no increase in `dbg/trace/debug`
- no increase in filesystem/helper surface (`OpenOptions|tempfile|rename(|fs::|std::fs`)
- no increase in `panic/todo/unimplemented`

Runbook:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave4-step1.sh
```

Optional (stacked/local override):
```bash
BASE_REF=<your-base-ref> bash scripts/ci/review-wave4-step1.sh
```

Definition of done:
- Freeze test subset green.
- Drift counters unchanged.
- Diff stays within allowlist.
