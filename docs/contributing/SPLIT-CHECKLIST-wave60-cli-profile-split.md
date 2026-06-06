# Wave60 CLI Profile Split Checklist

- [x] Confirm `origin/main` baseline before branching.
- [x] Keep `profile.rs` as a tiny facade.
- [x] Preserve public `profile` command API through re-exports.
- [x] Move event loading, aggregation, display, and tests without semantic edits.
- [x] Avoid Cargo, workflow, dependency, watch, run, dispatch, and args drift.
- [x] Add a scoped review gate for path allowlist, facade shape, module ownership, and profile tests.
- [ ] Run `BASE_REF=origin/main bash scripts/ci/review-wave60-cli-profile-split.sh`.
- [ ] Open PR only after local gate passes.
- [ ] Address review comments before merge.
- [ ] Merge only after GitHub checks are green and review threads are clean.
