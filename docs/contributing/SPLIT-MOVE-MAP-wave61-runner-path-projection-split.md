# Wave61 Runner Path Projection Move Map

| Before | After | Notes |
| --- | --- | --- |
| `path_projection.rs` public exports | `path_projection.rs` facade | Re-exported public API unchanged. |
| `PATH_PROJECTION_SCHEMA` | `path_projection_next/mod.rs` | Re-exported by facade. |
| `DeclaredPathProjectionRules` | `path_projection_next/mod.rs` | Rule storage/builders unchanged. |
| `DeclaredPathRule` | `path_projection_next/mod.rs` | Role constructors unchanged. |
| `PathProjection`, `PathProjectionMapping`, `UnmatchedPathSummary` | `path_projection_next/mod.rs` | Serialization shape unchanged. |
| `project_filesystem_paths` | `path_projection_next/project.rs` | Re-exported by `path_projection_next/mod.rs`. |
| Exact rule / prefix / operation helpers | `path_projection_next/project.rs` | Private helper behavior unchanged. |
| Inline `#[cfg(test)] mod tests` | `path_projection_next/tests.rs` | Moved contract tests. |

LOC delta:
- `crates/assay-runner-core/src/path_projection.rs`: 547 -> 7.
- New `path_projection_next/mod.rs`: 201.
- New `path_projection_next/project.rs`: 181.
- New `path_projection_next/tests.rs`: 178.

Deferred:
- No golden/schema fixture expansion in this PR.
- No runtime-noise taxonomy or path-equivalence semantic changes in this PR.
