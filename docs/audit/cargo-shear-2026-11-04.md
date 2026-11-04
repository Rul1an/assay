# Dependency candidate triage (#3480)

Baseline: `c8fd11101f06f0c6abf9cacc64e243926c682f8a`, cargo-shear 1.14.0,
macOS aarch64, `cargo-shear --locked --format json`. The scan reports 22 errors
(dependency candidates) and six warnings. It is a static heuristic, not proof
that removing a dependency preserves features, targets, macros or packaged builds.
No `--fix` run or permanent CI gate is introduced.

## Bounded change

`assay-policy` has no feature declarations, build script, examples or benches.
Its source and public signatures do not use `anyhow`, `thiserror` or `serde_yaml`
(the latter is the workspace alias for `yaml_serde`). Remove those three direct
edges. `serde_json` is used by `tests/cidr_rule_serde_shape.rs`, so move it to
`dev-dependencies`, retaining its `std` feature. No source or public API changes.
The lockfile removes only the three edges from `assay-policy`; it does not upgrade
or delete registry packages used elsewhere.

This move can change feature unification for downstream consumers: `assay-monitor`
uses `assay-common`, which requests `serde_json/alloc` rather than `std`.
A whole-workspace build can mask that difference. Verify the monitor separately,
including a normal dependency build (not only test targets), before landing.

## Candidate disposition

Counts refer to manifest entries, including the two target-specific entries for
`process-wrap`. "Retain" means not proven removable in this change, not that the
scanner finding is necessarily false.

| Package | Candidates | Count | Disposition |
|---|---|---:|---|
| `assay-policy` | `anyhow`, `thiserror`, `serde_yaml` | 3 | Remove: no source, macro or public API use. Verify existing tests and packaged crate. |
| `assay-policy` | `serde_json` | 1 | Move to dev-dependencies; preserve test feature and verify standalone consumers. |
| `assay-adapter-a2a`, `assay-adapter-acp`, `assay-adapter-ucp` | `serde` | 3 | Retain pending per-adapter tests and feature comparison; no direct Rust reference found. |
| `assay-cli` | `pkcs8`, `tracing` | 2 | Retain pending CLI target/feature tests. `pkcs8` appears through `ed25519_dalek` reexports; direct feature contribution must be checked. A local module named `tracing` is not proof of external-crate use. |
| `assay-core` | `globset`, `pkcs8`, `spki` | 3 | Retain pending consumer and signing tests. No `globset` reference found; crypto reexports/features need separate verification. |
| `assay-evidence` | `rand`, `uuid`, `assert_fs` | 3 | Retain pending crate/test/packaging checks; no direct Rust references found. |
| `assay-mcp-server` | `libc`, Unix and Windows `process-wrap` | 3 | Retain pending both platform paths and process-cleanup tests; no direct Rust references found. |
| `assay-registry` | `async-trait`, `pkcs8`, `url` | 3 | Retain pending standalone registry and signature tests. `pkcs8/pem` can contribute features through reexports; variables named `url` are not imports. |
| `assay-sim` | `tracing` | 1 | Retain pending simulator tests; no direct Rust reference found. |

The remaining warnings are separate work: optional `dirs` and `procfs` in
`assay-core` are wired to declared features; two unlinked source files and eleven
empty source files are not removed by this dependency change. A source deletion
needs module/target ownership analysis, not an unused-dependency heuristic.

## Replay and acceptance

Run the pinned scanner from the repository root, without `--fix`. After this
bounded change it should report 18 errors and six warnings; that remaining scan
is intentionally nonzero and is not a clean-workspace claim.

```sh
cargo-shear --version # 1.14.0
cargo-shear --locked --format json
cargo test -p assay-policy --locked
cargo test -p assay-policy --locked --no-default-features
cargo build -p assay-monitor --locked
cargo check -p assay-cli --locked
cargo clippy -p assay-policy --all-targets --all-features --locked -- -D warnings
cargo package -p assay-policy --locked
cargo fmt --all -- --check
```

Keep the scanner output, exact source SHA, toolchain, affected test results and
independent review together. Required hosted checks remain necessary for landing.
#3480 remains open for the other candidates; no suppressions or weaker gates are
needed to deliver this bounded cleanup.
