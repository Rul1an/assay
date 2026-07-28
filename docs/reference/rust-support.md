# Rust Support

Assay separates the minimum supported Rust version (MSRV) of its published crates from the
toolchain used to develop the full workspace.

## Published Crates

The public crate set defined by `scripts/ci/check-public-crate-policy.sh` supports Rust **1.89.0**
in the checked-in source tree. Each public package exposes `rust-version = "1.89"` through Cargo
metadata. This becomes a published-crate guarantee with the first release after 3.35.0; published
releases through 3.35.0 did not declare an MSRV.

CI checks every public workspace package, its default feature set, and all targets available on
the Linux CI host with Rust 1.89.0 against the workspace `Cargo.lock`. That build is the
compatibility check for the checked-in workspace graph, including local path dependencies.

For a source install that uses the lockfile shipped with the release selected by Cargo, use:

```bash
cargo install assay-cli --locked
```

The workspace CI lane does not claim that a packaged crate's lockfile is byte-identical to the
workspace lockfile. This policy also does not promise that an unconstrained future dependency
resolution will retain the same floor, or that target-specific dependencies unavailable on the
Linux CI host have been compiled there.

## Workspace Toolchain

Repository development uses Rust **1.96.0**, pinned in `rust-toolchain.toml`. That newer toolchain
drives formatting, clippy, tests, release work, and the full workspace. It does not silently raise
the published-crate MSRV.

The eBPF build uses its own dated nightly toolchain. That compiler is an internal build
requirement, not part of the public crate MSRV claim.

## Raising The Floor

An MSRV increase is an explicit compatibility decision. A change must update, in one pull request:

1. workspace and public-package `rust-version` metadata;
2. the pinned MSRV CI toolchain;
3. installation and developer documentation;
4. dependency holds whose rationale names the old floor.

Dependency convenience alone is not sufficient reason to raise the floor.
