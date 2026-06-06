# Wave61 Runner Path Projection Review Pack

Scope:
- Move-only split of runner-core path projection behind a stable facade.
- Review paths are limited to `path_projection.rs`, `path_projection_next/*`, Wave61 docs, and the Wave61 review gate.

Reviewer checks:
- `path_projection.rs` should only route to `path_projection_next` and re-export the existing public API.
- `path_projection_next/mod.rs` should own public schema/types, constants, and rule builders.
- `path_projection_next/project.rs` should own the projection algorithm and private mapping helpers.
- `path_projection_next/tests.rs` should contain the moved projection contract tests.

Required local gate:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave61-runner-path-projection-split.sh
```

Expected behavior:
- No path projection schema, serialization, non-claim, claim-level, ordering, operation-prefix, exact-rule precedence, or unmatched sampling changes.
- No Cargo/workflow/dependency drift.
- No archive/kernel/policy/SDK drift.

Known residual risk:
- The split is mechanical; residual risk is import-path equivalence and module visibility. Existing projection tests cover the high-value contract points.
