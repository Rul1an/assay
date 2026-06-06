# Wave62 Core Coverage Split Checklist

- [x] Confirm `origin/main` baseline before branching.
- [x] Keep `coverage.rs` as a tiny facade.
- [x] Preserve public `assay_core::coverage` re-exports.
- [x] Move coverage data contracts, analyzer logic, report formatting, and tests without semantic edits.
- [x] Avoid Cargo, workflow, dependency, `assay-core/src/lib.rs`, CLI, MCP server, baseline, and policy drift.
- [x] Add a scoped review gate for path allowlist, facade shape, module ownership, moved tests, and repo checks.
- [x] Run `BASE_REF=origin/main bash scripts/ci/review-wave62-core-coverage-split.sh`.
- [ ] Open PR only after local gate passes.
- [ ] Address review comments before merge.
- [ ] Merge only after GitHub checks are green and review threads are clean.
