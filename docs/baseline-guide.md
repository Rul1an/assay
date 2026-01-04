# Baseline Management Guide

Assay's **Baseline** feature allows you to "freeze" the expected behavior of your AI agent and detect regressions in subsequent runs. It works by comparing the metrics (coverage, semantic similarity) of a new run against a stored baseline.

## Workflow

1.  **Record**: Establish a baseline from a "good" run (e.g., on `main` branch).
2.  **Check**: Compare PR runs against the baseline.
3.  **Update**: If expectations change, update the baseline.

## Commands

### 1. Record a Baseline

Commit the current state of your agent's performance to a file.

```bash
# Saves latest run metrics to assay-baseline.json
assay baseline record

# Specify output file
assay baseline record --out .eval/baselines/main.json
```

**Tip:** Commit this file to Git to track the evolution of your quality gate.

### 2. Check for Regressions

Compare a new run against the recorded baseline.

```bash
assay baseline check

# Custom baseline path
assay baseline check --baseline .eval/baselines/main.json
```

Output Example:
```
Baseline comparison against run 124
❌ REGRESSIONS (1):
  - test_web_search metric 'score': 0.95 -> 0.40 (-0.55)

✅ No improvements.
```

### 3. CI Integration (JSON Output)

For automated pipelines, use the `--format json` flag to parse results programmatically.

```bash
assay baseline check --format json > report.json
```

## GitHub Actions Example

Use the baseline feature to block PRs that degrade performance.

```yaml
jobs:
  regression-gate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      # 1. Download baseline from 'main' branch artifact or previous commit
      - name: Download Baseline
        run: |
          # (Simplified) Fetch baseline JSON from storage
          wget https://my-storage.com/assay-baseline-main.json -O baseline.json

      # 2. Run Agent Tests
      - name: Run Tests
        run: assay run --strict --trace-file traces/current.jsonl

      # 3. Check Regression
      - name: Gate
        run: assay baseline check --baseline baseline.json --fail-on-regression
```

## Git Metadata Detection

Assay automatically captures Git context (Commit SHA, Branch, User) when recording baselines. In CI environments (like GitHub Actions), it automatically detects `GITHUB_SHA` and other environment variables if the `.git` directory is unavailable.

No extra configuration is needed!
