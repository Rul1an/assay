# PR4 Dependency and Perf Hygiene - 2026 Q2

## Scope

This note records the intentionally small PR4 cleanup slice from the repo hygiene memo.
It avoids functional parser migrations, API gates, mutation gates, security fixtures, and stale-code refactors.

Included:

- Remove the direct `sha256` crate dependency from `assay-core`; production hashing already uses the workspace `sha2` crate.
- Prune stale `deny.toml` entries that no longer match the resolved dependency graph.
- Capture the dependency snapshot so the review can see what changed and what remains intentionally out of scope.
- Create an explicit `serde_yaml` retirement plan without changing parser behavior in this PR.

Excluded:

- Migrating YAML parsing away from `serde_yaml`.
- Resolving ecosystem-wide duplicate transitive stacks such as `reqwest`, `rand`, `digest`, `getrandom`, and Windows target crates.
- Public API or semver gate changes.
- Security fixture changes.

## Before Snapshot

Command: `cargo tree -d`

Relevant findings before this PR:

- `assay-core` directly depended on `sha256 v1.6.0`.
- That direct dependency pulled an additional edge into `sha2 v0.10.9`, while the workspace already standardizes direct hashing on `sha2 v0.11.0`.
- `deny.toml` contained `RUSTSEC-2026-0097`, but `cargo deny check advisories bans sources` reported it as `advisory-not-detected`.
- `deny.toml` contained `https://github.com/aya-rs/aya` in `allow-git`, but `cargo deny` reported that exact source exception as unmatched. The lockfile still contains pinned `aya-rs/aya` git sources; they are covered by the repository's `unknown-git = "warn"` policy until a precise source allowlist is reintroduced.

## After Snapshot

Expected review outcomes after this PR:

- `Cargo.lock` no longer contains a `sha256` package entry.
- `crates/assay-core/Cargo.toml` no longer has a direct `sha256` dependency.
- `cargo tree -p assay-core -i sha256` no longer finds `sha256` in the resolved graph.
- `cargo deny check advisories bans sources` no longer reports the stale `RUSTSEC-2026-0097` ignore or the stale `aya-rs/aya` source allowlist.

Remaining duplicate dependency families are deliberately not changed in PR4 because they are transitive ecosystem transitions rather than a local direct dependency mistake:

- `sha2`/`digest` split between crypto dependencies using 0.10 and workspace code using 0.11.
- `rand`/`getrandom` split across older crypto stacks and newer storage/runtime stacks.
- `reqwest` split through `jsonschema` and direct HTTP clients.
- Windows target crate splits from platform-specific transitive dependencies.

## serde_yaml Retirement Plan

`serde_yaml` remains in place for PR4. It is deprecated upstream and should be retired in a dedicated compatibility migration, not as a drive-by dependency cleanup.

Candidate replacement direction:

- Evaluate `serde_yaml_ng` or another maintained YAML parser with compatible serde data-model behavior.
- Keep parsing behavior stable for lint packs, lockfiles, registry manifests, and user-authored config documents.
- Avoid parser replacement until golden compatibility tests exist.

Required migration evidence:

- Golden fixtures for representative pack YAML, lockfile YAML, config YAML, anchors/aliases if supported, duplicate-key behavior, scalar coercions, nulls, comments tolerance, and error messages that users see.
- Before/after parser behavior matrix documenting accepted input, rejected input, normalized values, and diagnostic drift.
- Explicit security review for entity expansion, alias abuse, deep nesting, resource limits, duplicate keys, and unexpected tag handling.
- Rollout plan that can either keep a compatibility fallback or intentionally reject ambiguous legacy YAML with clear migration guidance.

Exit criteria for a future parser migration PR:

- Existing golden vectors pass or intentional deltas are documented.
- User-facing diagnostics are at least as actionable as today.
- `cargo deny`/audit posture improves or stays neutral.
- The migration does not change Trust Basis, registry, or lint-pack semantics without a separate reviewed ADR.
