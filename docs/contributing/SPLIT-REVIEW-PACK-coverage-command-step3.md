# SPLIT REVIEW PACK — Wave19 Coverage Command Step3

## Intent
Close Wave19 with a docs+gate-only closure slice after the mechanical split of `crates/assay-cli/src/cli/commands/coverage.rs`.

Step3 must not move or edit runtime code.
It only:
- documents the closure,
- preserves reviewer context,
- re-runs Step2 invariants through a dedicated reviewer gate.

## Scope
Allowed files in this slice:
- `docs/contributing/SPLIT-CHECKLIST-coverage-command-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-coverage-command-step3.md`
- `scripts/ci/review-coverage-command-step3.sh`

Not allowed:
- `.github/workflows/*`
- `crates/assay-cli/src/cli/commands/coverage.rs`
- `crates/assay-cli/src/cli/commands/coverage/**`
- unrelated docs or release files

## What reviewers should verify
1. Diff is docs+script only.
2. Step3 gate re-runs the same structural invariants from Step2:
   - façade stays thin
   - `generate.rs` owns generator-mode logic
   - `legacy.rs` owns baseline/analyzer path
   - `io.rs` owns write/logging helpers only
3. Coverage contract tests still pass:
   - `coverage_contract`
   - `coverage_out_md`
   - `coverage_declared_tools_file`

## Reviewer commands

### Against stacked Step2 base
```bash
BASE_REF=origin/codex/wave19-coverage-command-step2-mechanical \
  bash scripts/ci/review-coverage-command-step3.sh
```

### Against origin/main after sync
```bash
BASE_REF=origin/main \
  bash scripts/ci/review-coverage-command-step3.sh
```

## Expected outcome
- Step3 adds no behavior changes
- closure remains diff-proof
- promote PR can be opened cleanly against main
