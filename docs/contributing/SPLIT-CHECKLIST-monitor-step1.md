# Monitor split Step 1 checklist (behavior freeze)

Scope lock:
- tests + docs + gates only
- no mechanical split yet
- no perf tuning
- `demo/` untouched

## Contract targets

- syntactic path normalization remains stable
- allow/not rule matching remains stable
- Linux syscall/unsafe footprint does not increase in Step 1

## Drift gates (hard-fail)

```bash
set -euo pipefail
BASE_REF="${BASE_REF:-origin/codex/wave2-step2-runtime-split}" bash scripts/ci/review-wave3-step1.sh
```

Canonical gate implementation lives in:
- `scripts/ci/review-wave3-step1.sh` (`strip_code_only`, drift counters, allowlist, and contract test selection).

Known limitation:
- The code-only filter in Step 1 is best-effort for `#[cfg(test)] mod tests { ... }` blocks.
- It will be replaced by stricter path/module-level filtering once tests are externalized in later wave steps.
- Drift gates are conservative: false positives are acceptable, false negatives are possible until tests are externalized.

Logging note:
- Step 1 intentionally enforces no-increase only for `println!/eprintln!`; log cleanup/reduction is out of scope for this step.

## Required contract tests

```bash
# Linux
cargo test -p assay-cli test_normalize_path_syntactic_contract -- --nocapture
cargo test -p assay-cli test_find_violation_rule_allow_not_contract -- --nocapture

# Non-Linux fallback checks
cargo test -p assay-cli test_normalize_path_syntactic_contract_skip_non_linux -- --nocapture
cargo test -p assay-cli test_find_violation_rule_allow_not_contract_skip_non_linux -- --nocapture
```

## Scope allowlist

```bash
BASE_REF="${BASE_REF:-origin/codex/wave2-step2-runtime-split}" bash scripts/ci/review-wave3-step1.sh
# includes a fail-fast diff allowlist gate for Step 1 scope
```

## Definition of done

- no drift-gate increases in `monitor.rs`
- monitor Step 1 contract tests pass
- scope lock respected
