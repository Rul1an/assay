# Wave62 Core Coverage Review Pack

Scope:
- Move-only split of `assay-core` coverage analysis behind a stable facade.
- Review paths are limited to `coverage.rs`, `coverage_next/*`, Wave62 docs, and the Wave62 review gate.

Reviewer checks:
- `coverage.rs` should only route to `coverage_next` and re-export the existing public API.
- `coverage_next/types.rs` should own the public report/metric/trace data contracts.
- `coverage_next/analyzer.rs` should own policy extraction, rule IDs, alias matching, coverage math, and high-risk gap detection.
- `coverage_next/report.rs` should own only `CoverageReport` formatting helpers.
- `coverage_next/tests.rs` should contain the moved coverage unit tests.

Required local gate:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave62-core-coverage-split.sh
```

Expected behavior:
- No coverage percentage, high-risk gap, alias resolution, unexpected tool, annotation, markdown, or serialization changes.
- No Cargo/workflow/dependency drift.
- No CLI, MCP server, baseline, policy, or `assay-core/src/lib.rs` drift.

Known residual risk:
- The split is mechanical; residual risk is import-path equivalence and module visibility. Existing coverage unit tests plus baseline determinism coverage imports cover the high-value contract points.
