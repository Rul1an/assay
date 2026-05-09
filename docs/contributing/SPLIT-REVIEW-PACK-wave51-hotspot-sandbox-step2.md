# SPLIT REVIEW PACK - Wave 51 Sandbox Step2

## Summary

Step 2 splits `assay sandbox` helper responsibilities into command-private modules while keeping `sandbox.rs::run` as the stable command facade.

## LOC Delta

| File | Before | After | Delta |
| --- | ---: | ---: | ---: |
| `crates/assay-cli/src/cli/commands/sandbox.rs` | 779 | 406 | -373 |
| `crates/assay-cli/src/cli/commands/sandbox/child.rs` | 0 | 169 | +169 |
| `crates/assay-cli/src/cli/commands/sandbox/degradation.rs` | 0 | 39 | +39 |
| `crates/assay-cli/src/cli/commands/sandbox/env.rs` | 0 | 29 | +29 |
| `crates/assay-cli/src/cli/commands/sandbox/profile.rs` | 0 | 136 | +136 |
| `crates/assay-cli/src/cli/commands/sandbox/tmp.rs` | 0 | 46 | +46 |

## Proof Snippets

Facade thinness:

```bash
awk 'BEGIN{n=0; in_tests=0} /^#\[cfg\(test\)\]/{in_tests=1} !in_tests{n++} END{print n}' crates/assay-cli/src/cli/commands/sandbox.rs
```

Boundary containment:

```bash
rg -n 'mod (child|degradation|env|profile|tmp);|run_child\(' crates/assay-cli/src/cli/commands/sandbox.rs
rg -n 'tokio::process::Command|timeout|TMPDIR|maybe_profile_finish' crates/assay-cli/src/cli/commands/sandbox/child.rs
rg -n 'EnvFilter::|with_strip_exec|with_allowed|with_safe_path' crates/assay-cli/src/cli/commands/sandbox/env.rs
rg -n 'PayloadSandboxDegraded|BackendUnavailable|PolicyConflict' crates/assay-cli/src/cli/commands/sandbox/degradation.rs
rg -n 'create_dir|remove_dir_all|set_permissions|0o700' crates/assay-cli/src/cli/commands/sandbox/tmp.rs
```

## Validation

- `cargo fmt --check`
- `cargo check -p assay-cli`
- `cargo clippy -p assay-cli --all-targets -- -D warnings`
- `cargo test -p assay-cli sandbox`
- `cargo test -p assay-cli --test profile_integration_test`
- `bash scripts/ci/review-wave51-hotspot-sandbox-step2.sh`

All listed checks passed locally for Step 2.
