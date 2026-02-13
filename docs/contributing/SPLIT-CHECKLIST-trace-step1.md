# Trace provider split Step 1 checklist (behavior freeze)

Scope lock:
- tests + docs + gates only
- no mechanical split yet
- no perf tuning
- `demo/` untouched

## Contract targets

- invalid trace line diagnostics remain stable (line context)
- v2 prompt/step precedence remains stable
- CRLF JSONL parsing behavior remains stable
- no unsafe footprint introduced

## Drift gates (hard-fail)

```bash
set -euo pipefail
base_ref="${BASE_REF:-origin/codex/wave2-step2-runtime-split}"
file="crates/assay-core/src/providers/trace.rs"
rg_bin="$(command -v rg)"

count_in_ref() {
  local pattern="$1"
  git show "${base_ref}:${file}" | awk 'BEGIN{in_tests=0} /^#\[cfg\(test\)\]/{in_tests=1} {if(!in_tests) print}' | "$rg_bin" -v '^[[:space:]]*//' | "$rg_bin" -n "$pattern" || true
}

count_in_worktree() {
  local pattern="$1"
  awk 'BEGIN{in_tests=0} /^#\[cfg\(test\)\]/{in_tests=1} {if(!in_tests) print}' "$file" | "$rg_bin" -v '^[[:space:]]*//' | "$rg_bin" -n "$pattern" || true
}

check_no_increase() {
  local pattern="$1"
  local label="$2"
  local before after
  before="$(count_in_ref "$pattern" | wc -l | tr -d ' ')"
  after="$(count_in_worktree "$pattern" | wc -l | tr -d ' ')"
  echo "$label: before=$before after=$after"
  if [ "$after" -gt "$before" ]; then
    echo "drift gate failed: $label increased"
    exit 1
  fi
}

check_no_increase "unwrap\\(|expect\\(" "trace unwrap/expect (code-only)"
check_no_increase "\\bunsafe\\b" "trace unsafe"
check_no_increase "println!\\(|eprintln!\\(" "trace println/eprintln (code-only)"
check_no_increase "panic!\\(|todo!\\(|unimplemented!\\(" "trace panic/todo/unimplemented (code-only)"
```

Known limitation:
- The code-only filter in Step 1 is best-effort for `#[cfg(test)] mod tests { ... }` blocks.
- It will be replaced by stricter path/module-level filtering once tests are externalized in later wave steps.
- Drift gates are conservative: false positives are acceptable, false negatives are possible until tests are externalized.

Logging note:
- Step 1 intentionally enforces no-increase only for `println!/eprintln!`; log cleanup/reduction is out of scope for this step.

## Required contract tests

```bash
cargo test -p assay-core --lib test_from_path_invalid_json_has_line_context -- --nocapture
cargo test -p assay-core --lib test_v2_non_model_prompt_is_only_fallback -- --nocapture
cargo test -p assay-core --lib test_from_path_accepts_crlf_jsonl_lines -- --nocapture
```

## Scope allowlist

```bash
BASE_REF="${BASE_REF:-origin/codex/wave2-step2-runtime-split}" bash scripts/ci/review-wave3-step1.sh
# includes a fail-fast diff allowlist gate for Step 1 scope
```

## Definition of done

- no drift-gate increases in `providers/trace.rs`
- trace provider contract tests pass
- scope lock respected
