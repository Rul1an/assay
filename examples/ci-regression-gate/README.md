# CI Regression Gate Demo (Offline + Deterministic)

This example demonstrates how to use Assay to prevent regressions in your CI pipeline without making live LLM calls.

## How it works
1.  **Baseline Export**: You record a "golden" trace (`traces/main.jsonl`) from your main branch and export a `baseline.json`.
2.  **PR Check**: When a PR is opened, you record a new trace (`traces/pr_bad.jsonl`) and Assay compares it against the baseline.
3.  **Strict Gating**: If the new trace degrades performance (e.g., changes "hello" to "goodbye"), Assay fails the build.

## Local Demo

### 1. Export Baseline (Simulate `main` branch)
Generate the baseline from the known-good trace:

```bash
chmod +x scripts/*.sh
./scripts/export_baseline_local.sh
```

**Output**: `✅ wrote examples/ci-regression-gate/baseline.json`

### 2. Gate PR (Simulate bad PR)
Run the gate against a trace that contains a regression:

```bash
./scripts/gate_pr_local.sh
```

**Expected Output**:
The script should report that the gate **CORRECTLY failed**, because `t_hello` returned "goodbye world" instead of containing "hello".

## GitHub Actions Integration

This directory includes two example workflows in `.github/workflows/`:
*   `assay-baseline-export.yml`: Updates the baseline on pushes to `main`.
*   `assay-pr-gate.yml`: Checks PRs against the baseline from the base branch.
