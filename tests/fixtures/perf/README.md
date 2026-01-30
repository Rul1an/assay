# Performance assessment fixtures

- **small:** `trace_small.jsonl` + `eval_small.yaml` (5 episodes, 5 tests, deterministic).
- **medium / large / worst:** Generated at runtime by `scripts/perf_assess.sh` into a temp dir (not committed).

Run full assessment (wall-clock, cold/warm, cleanup): from repo root, `./scripts/perf_assess.sh`. Requires `cargo build`.

See [PERFORMANCE-ASSESSMENT.md](../../../docs/PERFORMANCE-ASSESSMENT.md).
