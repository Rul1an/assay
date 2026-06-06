# Wave61 Runner Path Projection Split Checklist

- [x] Confirm `origin/main` baseline before branching.
- [x] Keep `path_projection.rs` as a tiny facade.
- [x] Preserve existing public re-exports from `assay-runner-core`.
- [x] Move schema/types, projection helpers, and tests without semantic edits.
- [x] Avoid Cargo, workflow, runner archive, kernel, policy, SDK, and dependency drift.
- [x] Add a scoped review gate for path allowlist, facade shape, module ownership, and projection tests.
- [ ] Run `BASE_REF=origin/main bash scripts/ci/review-wave61-runner-path-projection-split.sh`.
- [ ] Open PR only after local gate passes.
- [ ] Address review comments before merge.
- [ ] Merge only after GitHub checks are green and review threads are clean.
