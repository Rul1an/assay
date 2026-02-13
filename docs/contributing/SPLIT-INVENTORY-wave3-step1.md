# Wave 3 Step 1 inventory (behavior freeze)

Scope:
- `crates/assay-cli/src/cli/commands/monitor.rs`
- `crates/assay-core/src/providers/trace.rs`

Scope lock:
- tests + docs + gates only
- no split/mechanical moves yet
- no perf tuning
- `demo/` untouched

## HEAD snapshot

- commit: `9e46b1ee7e4de00cd85378ef04cbc566435d5b45`
- LOC:
  - `monitor.rs`: 895
  - `providers/trace.rs`: 881

## Public entrypoints (current)

`monitor.rs`
- `pub struct MonitorArgs`
- `pub async fn run(...)`

`providers/trace.rs`
- `pub struct TraceClient`
- `impl TraceClient { pub fn from_path(...) }`

## Baseline drift counters (Step 1, code-only)

Counters below exclude the `#[cfg(test)]` block in each file.

Current counts:
- `monitor.rs`
  - `unwrap(`: 2
  - `expect(`: 0
  - `unsafe`: 7
  - `println!/eprintln!`: 49
- `providers/trace.rs`
  - `unwrap(`: 0
  - `expect(`: 0
  - `unsafe`: 0
  - `println!/eprintln!`: 1

## Drift gates (copy/paste)

```bash
set -euo pipefail

base_ref="${BASE_REF:-origin/codex/wave2-step2-runtime-split}"
rg_bin="$(command -v rg)"

count_in_ref() {
  local ref="$1"
  local file="$2"
  local pattern="$3"
  git show "${ref}:${file}" | awk 'BEGIN{in_tests=0} /^#\[cfg\(test\)\]/{in_tests=1} {if(!in_tests) print}' | "$rg_bin" -v '^[[:space:]]*//' | "$rg_bin" -n "$pattern" || true
}

count_in_worktree() {
  local file="$1"
  local pattern="$2"
  awk 'BEGIN{in_tests=0} /^#\[cfg\(test\)\]/{in_tests=1} {if(!in_tests) print}' "$file" | "$rg_bin" -v '^[[:space:]]*//' | "$rg_bin" -n "$pattern" || true
}

check_no_increase() {
  local file="$1"
  local pattern="$2"
  local label="$3"
  local before after
  before="$(count_in_ref "$base_ref" "$file" "$pattern" | wc -l | tr -d ' ')"
  after="$(count_in_worktree "$file" "$pattern" | wc -l | tr -d ' ')"
  echo "$label: before=$before after=$after"
  if [ "$after" -gt "$before" ]; then
    echo "drift gate failed: $label increased"
    exit 1
  fi
}

check_no_increase "crates/assay-cli/src/cli/commands/monitor.rs" "unwrap\(|expect\(" "monitor unwrap/expect (code-only)"
check_no_increase "crates/assay-cli/src/cli/commands/monitor.rs" "\bunsafe\b" "monitor unsafe"
check_no_increase "crates/assay-cli/src/cli/commands/monitor.rs" "println!\(|eprintln!\(" "monitor println/eprintln (code-only)"
check_no_increase "crates/assay-cli/src/cli/commands/monitor.rs" "panic!\(|todo!\(|unimplemented!\(" "monitor panic/todo/unimplemented (code-only)"
check_no_increase "crates/assay-core/src/providers/trace.rs" "unwrap\(|expect\(" "trace unwrap/expect (code-only)"
check_no_increase "crates/assay-core/src/providers/trace.rs" "\bunsafe\b" "trace unsafe"
check_no_increase "crates/assay-core/src/providers/trace.rs" "println!\(|eprintln!\(" "trace println/eprintln (code-only)"
check_no_increase "crates/assay-core/src/providers/trace.rs" "panic!\(|todo!\(|unimplemented!\(" "trace panic/todo/unimplemented (code-only)"
```
