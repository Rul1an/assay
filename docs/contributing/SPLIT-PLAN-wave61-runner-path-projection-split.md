# Wave61 Runner Path Projection Split Plan

Goal:
- Mechanically split `crates/assay-runner-core/src/path_projection.rs` behind a stable facade.
- Keep path projection schema, mapping order, non-claims, claim levels, prefix rules, and unmatched sampling unchanged.
- Prioritize this runner/security-adjacent hotspot over larger test-only files.

Baseline:
- `crates/assay-runner-core/src/path_projection.rs`: 547 LOC on `origin/main` before Wave61.
- Existing inline tests cover declared workload roles, workdir prefixes, exact-rule precedence, unknown sampling, determinism, and non-equivalence claims.

Split shape:
- `path_projection.rs`: stable facade that re-exports the existing public API.
- `path_projection_next/mod.rs`: public schema/types, rule builders, constants, and module wiring.
- `path_projection_next/project.rs`: projection algorithm and private mapping helpers.
- `path_projection_next/tests.rs`: moved projection contract tests.

Non-goals:
- No schema, serialization, claim-level, non-claim, ordering, or sample-limit changes.
- No runner archive, kernel, policy, SDK, Cargo, workflow, or dependency changes.
- No public `assay-runner-core` API changes.

Review posture:
- Review as a move-only runner-core split.
- Any semantic projection changes belong in a follow-up PR with golden/schema fixtures.
