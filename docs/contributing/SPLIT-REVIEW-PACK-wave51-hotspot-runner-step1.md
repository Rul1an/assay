# SPLIT REVIEW PACK - Wave 51 Runner Step1

## Summary

Step 1 starts the 2026q2 hotspot refactor by finishing the existing `runner_next` direction. The runner facade keeps its public surface and delegates two remaining heavy implementation bodies to dedicated modules.

## LOC Delta

| File | Before | After | Delta |
| --- | ---: | ---: | ---: |
| `crates/assay-core/src/engine/runner.rs` | 696 | 462 | -234 |
| `crates/assay-core/src/engine/runner_next/assertions.rs` | 0 | 49 | +49 |
| `crates/assay-core/src/engine/runner_next/single.rs` | 0 | 193 | +193 |

## Proof Snippets

Facade thinness:

```bash
awk 'BEGIN{n=0; in_tests=0} /^#\[cfg\(test\)\]/{in_tests=1} !in_tests{n++} END{print n}' crates/assay-core/src/engine/runner.rs
```

Boundary containment:

```bash
rg -n 'runner_next::(assertions::apply_agent_assertions_impl|single::run_test_once_impl)' crates/assay-core/src/engine/runner.rs
```

Moved implementation:

```bash
rg -n 'cache_key|assay.eval.metric|verify_assertions' crates/assay-core/src/engine/runner_next
```

## Validation

- `cargo fmt`
- `cargo check -p assay-core`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- `cargo test -p assay-core --lib runner_contract_`
- `cargo test -p assay-core --test runner_metric_spans`
- `bash scripts/ci/review-wave51-hotspot-runner-step1.sh`

All listed checks passed locally for Step 1.
