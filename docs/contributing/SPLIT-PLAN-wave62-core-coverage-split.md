# Wave62 Core Coverage Split Plan

Scope:
- Move-only split of `crates/assay-core/src/coverage.rs` behind a stable facade.
- Preserve the public `assay_core::coverage::{...}` API through re-exports.
- Keep behavior, serialization shape, threshold math, annotations, markdown output, and tests unchanged.

Implementation:
- `coverage.rs`: tiny public facade that routes to `coverage_next`.
- `coverage_next/types.rs`: public coverage data contracts and trace record.
- `coverage_next/analyzer.rs`: policy extraction, alias matching, coverage math, and high-risk gap detection.
- `coverage_next/report.rs`: GitHub annotation and markdown formatting.
- `coverage_next/tests.rs`: moved coverage unit tests.

Non-goals:
- No Cargo, workflow, dependency, edition, or lint posture changes.
- No policy coverage semantics changes.
- No evidence, CLI, MCP server, baseline, or policy module edits.
- No new golden fixture expansion in this PR.

Required gate:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave62-core-coverage-split.sh
```
