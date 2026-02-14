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

## Baseline drift counters (Step 1, best-effort code-only)

Counters below exclude the `#[cfg(test)]` block in each file.
Counters are based on the current filter rules in `scripts/ci/review-wave3-step1.sh`.

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
BASE_REF="${BASE_REF:-origin/codex/wave2-step2-runtime-split}" bash scripts/ci/review-wave3-step1.sh
```

Canonical gate implementation lives in:
- `scripts/ci/review-wave3-step1.sh` (`strip_code_only`, drift counters, allowlist, and contract test selection).
