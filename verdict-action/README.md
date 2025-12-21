# Verdict GitHub Action

Official GitHub Action for [Verdict](https://github.com/Rul1an/verdict), the deterministic regression testing tool for LLM pipelines.

![License](https://img.shields.io/github/license/Rul1an/verdict)
![Version](https://img.shields.io/github/v/release/Rul1an/verdict)

## Features
- **Replay Support**: Run tests deterministically using pre-recorded traces.
- **Reporting**: Automatically uploads JUnit and SARIF reports.
- **Quarantine**: Respects quarantine status for failing tests.
- **Baselines**: Compare PRs against a "known good" baseline from main.

## Usage

### Basic Usage (Replay Mode)
```yaml
- uses: Rul1an/verdict-action@v1
  with:
    verdict_version: v0.1.0
    config: ci-eval.yaml
    trace_file: traces/ci.jsonl
```

### Baseline Gating (Recommended)
This workflow ensures that a PR does not regress compared to the `main` branch.

**1. PR Gate (compare against main)**
Fetches the baseline from the main branch and gates the PR against it.
```yaml
- name: Get baseline from main
  shell: bash
  run: |
    git fetch origin main:refs/remotes/origin/main
    git show origin/main:baselines/ci.baseline.json > baseline.json

- uses: Rul1an/verdict-action@v1
  with:
    verdict_version: v0.1.0
    config: ci-eval.yaml
    trace_file: traces/ci.jsonl
    baseline: baseline.json
    strict: "true"
```

**2. Export Baseline (on main)**
Runs tests on `main` and (optionally) exports the updated baseline.
```yaml
- uses: Rul1an/verdict-action@v1
  with:
    verdict_version: v0.1.0
    config: ci-eval.yaml
    trace_file: traces/ci.jsonl
    export_baseline: baselines/ci.baseline.json
    upload_exported_baseline: "true"
    exported_baseline_artifact_name: "ci-baseline"
```

## Inputs

| Input | Description | Default |
| :--- | :--- | :--- |
| `repo` | Repository to download Verdict binary from. | `Rul1an/verdict` |
| `verdict_version` | **Required**. Release tag to download (e.g. `v0.1.0`). | |
| `config` | Path to eval config YAML. | `ci-eval.yaml` |
| `trace_file` | Path to JSONL trace file (activates Replay Mode). | `""` |
| `baseline` | Path to baseline JSON for regression checking. | `""` |
| `export_baseline` | Path to write new baseline JSON to. | `""` |
| `upload_exported_baseline` | Upload exported baseline as artifact? | `false` |
| `strict` | If true, Warn/Flaky tests cause exit code 1. | `false` |
| `working_directory` | Directory to run verdict in. | `.` |

## Permissions
If enabling SARIF upload (`upload_sarif: true`), you must grant:
```yaml
permissions:
  security-events: write
```

## Outputs
- `junit_path`: Path to generated JUnit XML.
- `sarif_path`: Path to generated SARIF JSON.
- `baseline_path`: Resolved path to input baseline.
- `exported_baseline_path`: Resolved path to exported baseline.
