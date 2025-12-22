# Verdict User Guide

**Verdict** is a strict, CI-first regression gate for RAG pipelines (and other LLM apps). It is designed to be deterministic, fast, and easy to integrate into Pull Request workflows.

## 🚀 Quickstart

1. **Install Verdict** (via `cargo` or pre-built binary):
   ```bash
   cargo install --git https://github.com/Rul1an/verdict.git verdict-cli
   ```

2. **Initialize Scaffolding**:
   Run `init` to generate a ready-to-use CI setup, including a sample config and traces for deterministic replay.
   ```bash
   verdict init --ci --gitignore
   ```
## Quick Links
- [Configuration Reference](#configuration-reference)
- [Command Line Interface](#command-line-interface)
- [Troubleshooting](./TROUBLESHOOTING.md)

   This creates:
   - `ci-eval.yaml`: Evaluation configuration.
   - `traces/ci.jsonl`: Pre-recorded LLM interactions (Replay Mode).
   - `schemas/ci_answer.schema.json`: Example JSON Schema.
   - `.github/workflows/verdict.yml`: GitHub Actions workflow.

3. **Run CI Gate**:
   ```bash
   verdict ci --config ci-eval.yaml --trace-file traces/ci.jsonl --strict
   ```

---

## 💡 Core Concepts

### Statuses
Verdict tests result in one of five statuses:
- **Pass**: Metric matched expectation.
- **Fail**: Metric failed (e.g. regex didn't match).
- **Error**: System error (e.g. LLM call failed, config error, trace missing).
- **Warn**: Test failed, but is marked as `warn` in Quarantine (Non-blocking by default).
- **Flaky**: Test passed sometimes and failed sometimes (Auto-rerun detected).

### Strict Mode (`--strict`)
By default, `Warn` and `Flaky` statuses are treated as passing (Exit Code 0).
Use `--strict` to treat them as failures (Exit Code 1), enforcing a clean green state.

### Replay Mode vs Reruns
- **Replay Mode**: When `--trace-file` is provided, Verdict uses recorded LLM responses. This allows for **deterministic** and **fast** CI runs without API costs.
- **Reruns**: In live mode, Verdict can retry failed tests (`--rerun-failures N`) to detect flakiness. In Replay Mode, reruns are forced to 0.

### Path Resolution
File paths in configuration (e.g., `schema_file`) are resolved **relative to the configuration file**. This ensures your config works consistently whether run from the project root or a subdirectory.

---

## 📝 Configuration (`eval.yaml`)

```yaml
version: 1
suite: "my_rag_suite"
model: "gpt-4o" # or "trace" for replay
tests:
  - id: "rag_q1"
    input:
      prompt: "Explain RAG"
    expected:
      # Option A: Regex Match
      type: regex_match
      pattern: "retrieval.*generation"
      flags: ["i"]

  - id: "json_output"
    input:
      prompt: "Output JSON"
    expected:
      # Option B: JSON Schema
      type: json_schema
      schema_file: "schemas/answer.schema.json" # Relative to eval.yaml
```

### Metrics
- `must_contain`: List of substrings that must be present.
- `must_not_contain`: List of substrings that must be absent.
- `regex_match` / `regex_not_match`: Perl-style regex validation.
- `json_schema`: Validates structure against a JSON Schema (inline or file).
- `semantic_similarity_to`: Fuzzy match using vector embeddings (requires `openai` embedder or trace metadata).

### Semantic Similarity (Fuzzy Match)
Use this metric to check if the output "means" the same as a reference string, even if the wording differs.

```yaml
    expected:
      type: semantic_similarity_to
      text: "The user is allowed to reset their password."
      threshold: 0.85 # (Default: 0.80)
```

**Running Locally (Live)**:
Requires `OPENAI_API_KEY`.
```bash
export OPENAI_API_KEY=sk-...
verdict run --embedder openai --embedding-model text-embedding-3-small
```

**Running in CI (Offline/Replay)**:
If you are using `--trace-file`, Verdict will look for pre-computed embeddings in the matching trace entry `meta.verdict.embeddings`.
This makes semantic tests **deterministic and free** in CI. No API calls are made if embeddings are present in the trace.

**Trace Schema (v1)**:
```json
{
  "request_id": "...",
  "prompt": "...",
  "response": "...",
  "meta": {
    "verdict": {
      "embeddings": {
        "model": "text-embedding-3-small",
        "response": [0.1, 0.2, ...],
        "reference": [0.3, 0.4, ...],
        "source_response": "live",
        "source_reference": "live"
      }
    }
  }
}
```

---

## 🛡️ Privacy & Security

### `--redact-prompts`
Use this flag to redact the `prompt` field in all generated artifacts (SARIF, JSON, JUnit). This prevents PII or sensitive data from leaking into CI logs.

```bash
verdict ci ... --redact-prompts
```

---

## 📊 CI/CD Integration

Verdict is designed for GitHub Actions (and other CI systems).

### Reports
- `junit.xml`: Test results for CI UI integration. `Warn`/`Flaky` tests appear as "Passed" with warning logs (unless `--strict`).
- `sarif.json`: Static Analysis results for GitHub Code Scanning (showing failures inline in PRs).
- `run.json`: Full detailed JSON report.

### Workflow Example
See `.github/workflows/verdict.yml` generated by `verdict init --ci`.

---

## 🔧 Troubleshooting

- **"config error: failed to read schema_file"**: Verdict prints both the *resolved absolute path* and the *original relative path* to help you debug file location issues in CI.
- "trace miss": In Replay Mode, this means the prompt requested is not found in the provided --trace-file.

---

---

## 📈 Calibration (Metrics Analysis)

Stop guessing metric thresholds. Use `calibrate` to analyze previous test runs and get statistical recommendations.

### Usage

1. **Run tests** (to generate data):
   ```bash
   verdict ci --export-db .eval/eval.db
   ```

2. **Run Calibration Report**:
   ```bash
   verdict calibrate --db .eval/eval.db --suite my_rag_suite --out calibration.json
   ```

### Output Example (`calibration.json`)
```json
"metrics": [
  {
    "key": { "metric": "semantic_similarity_to" },
    "p10": 0.82, "p50": 0.89, "p90": 0.94,
    "recommended_min_score": 0.82,
    "recommended_max_drop": 0.07
  }
]
```
Use these values to tune your `eval.yaml` thresholds or set realistic baselines.

---

## 📉 Baseline Workflow (Regression Testing)

For critical applications, defining absolute thresholds (e.g. "score > 0.8") is hard. It is often better to ensure that new changes **do not degrade** performance compared to the `main` branch.

### 3-Step Workflow

**1. Export Baseline (on main)**
In your CI pipeline for the `main` branch, run the tests and export the results as a baseline artifact.

```bash
verdict ci --export-baseline baseline.json
# Upload baseline.json as a CI artifact
```

**2. Compare (on PRs)**
In your Pull Request pipeline, download the baseline artifact from `main` and run the tests against it.

```bash
# Download baseline.json
verdict ci --baseline baseline.json
```

**3. Configure Thresholds**
In your `eval.yaml`, configure `thresholding` to define acceptable regression.

```yaml
settings:
  thresholding:
    mode: relative
    max_drop: 0.01  # Allow 1% drop
    # min_floor: 0.7 # Optional safety net
```

If a test score drops by more than `0.01` compared to the baseline, the test will **Fail**.

### GitHub Actions (Golden Path)

Use the official `verdict-action` which handles artifacts for you.

**1. Main Branch (Export)**
```yaml
- uses: verdict-eval/action@v1
  with:
    verdict_version: v0.2.1
    export_baseline: baseline.json # generates and uploads 'verdict-baseline' artifact
```

**2. Pull Requests (Gate)**
```yaml
# Fetch baseline from artifact (or commit)
- uses: actions/download-artifact@v4
  with:
    name: verdict-baseline
    path: .
  continue-on-error: true

- uses: verdict-eval/action@v1
  with:
    verdict_version: v0.2.1
    baseline: baseline.json # runs comparison if file exists
```

---

## 🔧 Troubleshooting

### Common Issues
- **"config error: failed to read schema_file"**: Check if the path is relative to `eval.yaml`.
- **"trace miss"**: The prompt requested is not found in the provided `--trace-file`.

### Baseline Errors
- **"config error: baseline suite mismatch (Exit 2)"**: You are trying to compare results against a baseline generated from a different test suite. Ensure you are downloading the correct artifact.
- **"config error: unsupported baseline schema version (Exit 2)"**: The baseline was generated by a newer/older version of Verdict that is incompatible. Regenerate the baseline on `main`.
- **"warning: config fingerprint mismatch"**: The `eval.yaml` used to generate the baseline differs from the current configuration. This is expected if you just changed the config in your PR. The warning reminds you that the comparison might be noisy until the baseline is updated.

