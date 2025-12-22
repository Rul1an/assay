# Verdict

Verdict is a **local-first** evaluation and regression-gating tool for LLM apps (RAG, agents, assistants).
It’s optimized for **deterministic replay in CI**, **baseline regression detection**, and
- **[User Guide](./docs/user-guide.md)**: Full configuration reference
- **[Troubleshooting](./docs/TROUBLESHOOTING.md)**: Fast path for fixing CI issues
- **[Installation](./docs/install.md)**: Setup guided (CI): GitHub Action
Use the GitHub Action so you **don’t need a Rust toolchain**.

```yaml
- uses: Rul1an/verdict-action@v0.3.4
  with:
    config: eval.yaml
    trace_file: traces/ci.jsonl
```

### Local (dev)

If you have Rust installed:

```bash
cargo install verdict-cli
verdict --version
```

## Core concepts
- **Trace file** (`.jsonl`): captured requests/responses for deterministic replay.
- **Metrics**: deterministic checks, embeddings-based similarity, and optional LLM-as-judge.
- **Baselines**: compare against a “known-good” run (relative thresholds) to catch regressions.

## Common workflows

### 1) Deterministic CI gate (baseline regression)

**Main branch**: export baseline once (or update when behavior intentionally changes):

```bash
verdict ci --config eval.yaml --trace-file traces/main.jsonl --export-baseline baseline.json --strict
git add baseline.json && git commit -m "Update eval baseline"
```

**PRs**: gate against the committed baseline:

```bash
verdict ci --config eval.yaml --trace-file traces/pr.jsonl --baseline baseline.json --strict
```

**GitHub Action example**:

```yaml
- uses: Rul1an/verdict-action@v0.3.4
  with:
    config: eval.yaml
    trace_file: traces/pr.jsonl
    baseline: baseline.json
```

### 2) Offline CI, cost-safe (strict replay)

Use this when CI must be deterministic and must not make network calls.

```bash
# 1) Normalize logs into a trace dataset (optional)
verdict trace ingest --input raw_logs/*.jsonl --output trace.jsonl

# 2) Precompute everything needed
verdict trace precompute-embeddings --trace trace.jsonl --output trace.enriched.jsonl --embedder openai
verdict trace precompute-judge      --trace trace.enriched.jsonl --output trace.enriched.jsonl --judge openai

# 3) Run CI fully offline
verdict ci --config eval.yaml --trace-file trace.enriched.jsonl --replay-strict
```

If required data is missing (e.g., new semantic tests added), strict replay exits with code 2 and a clear instruction.

### 3) Tune thresholds with data (calibration)

```bash
# from a run artifact
verdict calibrate --run run.json --out calibration.json

# or from DB history
verdict calibrate --db .eval/eval.db --suite my_suite --last 200 --out calibration.json
```

### 4) Find flaky/unstable tests (hygiene report)

```bash
verdict baseline report --db .eval/eval.db --suite my_suite --last 50 --out hygiene.json
```
