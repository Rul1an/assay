# SPLIT PLAN - Week 8 SOTA Gates 2026q2

## Intent

Add optional, high-signal gates for refactor waves after the Wave51 hotspot split. These gates are intentionally opt-in or documentation-backed first, because the required tools are not guaranteed to be installed on every contributor machine.

## Source Baseline

- Cargo's SemVer compatibility guide classifies renaming, moving, or removing public items as a major compatibility change: <https://doc.rust-lang.org/cargo/reference/semver.html>
- `cargo-semver-checks` scans Rust crates for SemVer violations: <https://github.com/obi1kenobi/cargo-semver-checks>
- `cargo-public-api` lists and diffs public APIs from rustdoc JSON and can be used in CI: <https://github.com/cargo-public-api/cargo-public-api>
- OWASP MCP Top 10 v0.1 names the MCP security categories that map to Assay proxy, sandbox, and Trust Basis tests: <https://owasp.org/www-project-mcp-top-10/>
- `cargo-mutants` supports workspace/package mutation testing with timeouts and package selection: <https://mutants.rs/workspaces.html>

## Gate 1 - Public API Drift

Script:

```bash
bash scripts/ci/optional-public-api-drift.sh
```

Behavior:

- Runs `cargo semver-checks check-release` for library crates when `cargo-semver-checks` is installed.
- Runs `cargo public-api` diff checks when `cargo-public-api` is installed and supports package-scoped diffs.
- Skips with an explicit notice when tools are unavailable.

Initial package set:

- `assay-core`
- `assay-evidence`
- `assay-registry`
- `assay-policy`
- `assay-metrics`

Rule:

- Required for release candidates and public facade refactors.
- Optional/non-blocking for early mechanical split PRs until CI has the tool cache.

## Gate 2 - Mutation Smoke on Pure Modules

Script:

```bash
ASSAY_RUN_MUTATION_SMOKE=1 bash scripts/ci/mutation-smoke-pure-modules.sh
```

Initial target modules:

- `crates/assay-evidence/src/trust_basis/diff.rs`
- `crates/assay-evidence/src/trust_basis/classifiers.rs`
- `crates/assay-cli/src/cli/commands/sandbox/degradation.rs`

Rule:

- Keep mutation smoke targeted to pure or near-pure modules.
- Do not run full-workspace mutation testing in PR CI.
- Use this to catch weak assertions after module extraction, especially where branch logic was moved mechanically.

## Gate 3 - OWASP MCP Test Mapping

Mapping:

- `docs/security/OWASP-MCP-TOP10-TEST-MAP.md`

Required focus categories for the current MCP/security refactor lineage:

- MCP01 token/secret exposure
- MCP02 scope creep
- MCP03 tool poisoning
- MCP05 command injection/execution
- MCP06 prompt/context/intent injection
- MCP08 audit and telemetry gaps
- MCP10 context over-sharing

Rule:

- New proxy, sandbox, or Trust Basis tests should name which MCP risk they cover or explicitly state that they are not security coverage tests.
- Coverage claims should reference concrete tests, not just product capabilities.
