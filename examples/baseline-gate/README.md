# Baseline Gate Example

This example demonstrates how to use **Baselines** in CI to detect regressions.

## Files
- `eval.yaml`: Configuration (uses `model: trace` for offline replay).
- `baseline.json`: The "known good" state (Golden Baseline).
- `traces/run.jsonl`: The recorded trace outputs.

## Usage

### 1. Verify against Baseline (CI Gate)
Blocks the build if scores drop below the baseline.

```bash
verdict run --config eval.yaml --trace-file traces/run.jsonl --baseline baseline.json
```

### 2. Export New Baseline (Update)
Updates the baseline on `main` branch.

```bash
verdict run --config eval.yaml --trace-file traces/run.jsonl --export-baseline baseline.json
```
