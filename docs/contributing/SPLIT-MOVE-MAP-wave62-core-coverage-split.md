# Wave62 Core Coverage Move Map

| Before | After | Notes |
| --- | --- | --- |
| `coverage.rs` public exports | `coverage.rs` facade | Re-exported public API unchanged. |
| Coverage data structs | `coverage_next/types.rs` | Serialization derives and field names unchanged. |
| `TraceRecord` | `coverage_next/types.rs` | Public trace input shape unchanged. |
| `CoverageAnalyzer` | `coverage_next/analyzer.rs` | Public constructor and `analyze` method unchanged. |
| Policy extraction and rule ID helpers | `coverage_next/analyzer.rs` | Private helper behavior unchanged. |
| Alias matching and seen-tool helpers | `coverage_next/analyzer.rs` | Private helper behavior unchanged. |
| `CoverageReport` formatters | `coverage_next/report.rs` | GitHub annotation and markdown output unchanged. |
| Inline `#[cfg(test)] mod tests` | `coverage_next/tests.rs` | Moved coverage unit tests. |

LOC delta:
- `crates/assay-core/src/coverage.rs`: 615 -> 12.
- New `coverage_next/mod.rs`: 19.
- New `coverage_next/types.rs`: 101.
- New `coverage_next/analyzer.rs`: 282.
- New `coverage_next/report.rs`: 81.
- New `coverage_next/tests.rs`: 144.

Deferred:
- No policy coverage semantics changes in this PR.
- No golden serialization fixture expansion in this PR.
- No CLI or MCP coverage command split in this PR.
