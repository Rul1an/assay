# SPLIT-REVIEW-PACK-wave3-step1

Generated from probe run at `2026-02-13 20:54:28Z` (UTC).
Update this file only when behavior-freeze tests, drift gates, or Step 1 scope changes.

## PR + stack

- PR: https://github.com/Rul1an/assay/pull/337
- Base: `codex/wave2-step2-runtime-split`
- Head: `codex/wave3-step1-behavior-freeze-v2`
- Scope: behavior freeze only (no mechanical split/perf work)

## Commit slices

1. `050c99c2` docs(wave3-step1): add inventory, checklists and reviewer script
2. `e0baca5e` test(wave3-step1): freeze monitor normalization and rule-match contracts
3. `5ece2dc6` docs(wave3-step1): harden drift gates and scope allowlist
4. `9e46b1ee` docs(wave3-step1): filter comment lines in drift counters
5. `9494207f` docs(wave3-step1): sync inventory snapshot and drift counts

## Review artifacts

- `docs/contributing/SPLIT-INVENTORY-wave3-step1.md`
- `docs/contributing/SPLIT-SYMBOLS-wave3-step1.md`
- `docs/contributing/SPLIT-CHECKLIST-monitor-step1.md`
- `docs/contributing/SPLIT-CHECKLIST-trace-step1.md`
- `scripts/ci/review-wave3-step1.sh`

## Freeze contracts (what must not drift)

Monitor hotspot (`crates/assay-cli/src/cli/commands/monitor.rs`):
- path normalization semantics
- allow/not rule matching behavior
- Linux syscall/unsafe footprint non-increase in Step 1

Trace hotspot (`crates/assay-core/src/providers/trace.rs`):
- invalid line diagnostics keep line context
- v2 prompt/step precedence keeps fallback semantics
- CRLF JSONL parsing remains accepted

## Hard gates (copy/paste)

```bash
bash scripts/ci/review-wave3-step1.sh
```

Script enforces:
- quality checks: `fmt`, `clippy`, `check`
- targeted contract tests (monitor + trace)
- drift no-increase counters for:
  - `unwrap/expect`
  - `unsafe`
  - `println!/eprintln!`
  - `panic!/todo!/unimplemented!`
- scope allowlist hard-fail for Step 1 files
- stacked base override support (`BASE_REF=...`)

## Probe results from latest run

- monitor drift:
  - unwrap/expect `2 -> 2`
  - unsafe `7 -> 7`
  - println/eprintln `49 -> 49`
  - panic/todo/unimplemented `0 -> 0`
- trace drift:
  - unwrap/expect `0 -> 0`
  - unsafe `0 -> 0`
  - println/eprintln `1 -> 1`
  - panic/todo/unimplemented `0 -> 0`
- allowlist gate: `PASS`

## Public symbol snapshot

`monitor.rs`
- `pub struct MonitorArgs`
- `pub async fn run(args: MonitorArgs) -> anyhow::Result<i32>`

`providers/trace.rs`
- `pub struct TraceClient`
- `pub fn from_path<P: AsRef<Path>>(path: P) -> anyhow::Result<Self>`

## CI status at generation time

- PR checks summary: `COMPLETED=20`, `IN_PROGRESS=11`
- Merge readiness: once CI is fully green

## Known limitations (explicit)

- Drift gates are conservative. False positives are acceptable; false negatives remain possible until tests are externalized from hotspot files.
- Step 1 intentionally keeps `println!/eprintln!` as no-increase only; log cleanup is deferred to later step(s).
