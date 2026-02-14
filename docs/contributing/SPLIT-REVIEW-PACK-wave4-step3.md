# Review Pack: Wave4 Step3 (`explain.rs` mechanical split)

Intent:
- Mechanically split `explain.rs` into focused modules behind a stable `crate::explain` facade.

Scope:
- `crates/assay-core/src/explain.rs`
- `crates/assay-core/src/explain_next/*`
- `docs/contributing/SPLIT-{MOVE-MAP,CHECKLIST,REVIEW-PACK}-wave4-step3.md`
- `scripts/ci/review-wave4-step3.sh`
- `docs/architecture/PLAN-split-refactor-2026q1.md`

Verification command:
```bash
BASE_REF=origin/codex/wave4-step2x-cache-thinness bash scripts/ci/review-wave4-step3.sh
```

Executed and passing:
- `cargo fmt --check`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- `cargo check -p assay-core`
- explain anchors:
  - `test_explain_simple_trace`
  - `test_explain_blocked_trace`
  - `test_explain_max_calls`
  - `test_terminal_output`
- facade gates + single-source gates
- diff allowlist

No behavior/perf changes intended:
- no public path renames
- no output/error semantics rewrites
- no dependency/Cargo changes
