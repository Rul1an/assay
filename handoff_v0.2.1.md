# Project Handoff: Verdict v0.2.1

**Verdict** is a strict, CI-first regression testing framework for LLM-based applications (RAG chains, Agents). It is designed to replace "vibe checks" with deterministic, automated gates in Pull Requests.

## 1. System Overview

- **Version**: v0.2.1 (Design Partner Pilot Ready)
- **Language**: Rust (Safe, Fast, Single Binary)
- **Distribution**:
  - **Binary**: GitHub Releases (Linux x86_64-musl, macOS x86_64/arm64)
  - **Action**: `verdict-eval/action@v1`

## 2. Key Capabilities

### A. Evaluation Metrics
Verdict supports a layered metric strategy:
1.  **Deterministic (Fast/Free)**:
    -   `must_contain` / `regex_match`: Text validity.
    -   `json_schema`: Structure validation.
2.  **Semantic (Embedding-based)**:
    -   `semantic_similarity_to`: Fuzzy matching against reference answers.
    -   *Note*: Uses `openai` embeddings (text-embedding-3-small) or cached trace data.
3.  **Model-Graded (LLM-as-a-Judge)**:
    -   `faithfulness` / `relevance`: Uses an LLM to grade output quality.

### B. CI-First Design
-   **Replay Mode**: Use `--trace-file traces.jsonl` to run evaluations offline. Verdict mocks the LLM responses using the trace data, making CI **deterministic** and **cost-free**.
-   **Strict Gating**:
    -   `--strict`: Fails CI if any test is `Warn` or `Flaky`.
    -   `--quarantine-mode`: Suppresses known failures to keep pipelines green.
-   **Standard Outputs**: Generates `junit.xml` (for CI UI), `sarif.json` (for GitHub Code Scanning), and `run.json` (for analysis).

### C. Regression Baseline System (v0.2 feature)
Verdict prevents regressions by comparing the current run against a "known good" baseline from the `main` branch.
-   **Export**: `verdict ci --export-baseline baseline.json` (on Main).
-   **Gate**: `verdict ci --baseline baseline.json` (on PR).
-   **Hardening**: Enforces strict schema matching (Exit Code 2 on mismatch) to prevent silent configuration drift.

## 3. Architecture

The codebase is a Cargo workspace organized into:

| Crate | Purpose | Key Components |
| :--- | :--- | :--- |
| `crates/verdict-cli` | The binary entrypoint | CLI parsing (Clap), `cmd_run`, `cmd_ci`, output handling. |
| `crates/verdict-core` | The engine | `Runner`, `Baseline`, `Provider` (OpenAI/Trace), `Redaction`. |
| `crates/verdict-metrics`| Metric logic | `Metric` trait, `RegexMetric`, `SemanticMetric`, `JsonSchemaMetric`. |
| `verdict-action` | GitHub Action | Composite action wrapping the binary download & artifacts. |

### Build System
-   **Cross-Compilation**: Uses `cross` (Docker-based) for `x86_64-unknown-linux-musl` to ensure portability across Linux distros.
-   **TLS**: Uses `rustls` (via `reqwest`) to avoid OpenSSL linking headaches on Linux.

## 4. Usage Guide

### Installation
```bash
cargo install --git https://github.com/Rul1an/verdict.git verdict-cli
# Or download binary from GitHub Releases
```

### Initializing a Project
```bash
verdict init --ci --gitignore
# Generates ci-eval.yaml, traces/, schemas/, and GitHub Workflow
```

### Running the Gate
```bash
# Live Run (requires API Key)
verdict run --config eval.yaml

# Replay Run (CI safe)
verdict ci --config eval.yaml --trace-file traces/ci.jsonl --strict
```

## 5. Maintenance & Release

### How to Release
1.  **QA**: Ensure `cargo test` passes and `verdict-action/action.yml` matches `release.yml` logic.
2.  **Tag**: `git tag v0.2.x && git push origin v0.2.x`.
3.  **Automation**: The GitHub Action `.github/workflows/release.yml` will:
    -   Build binaries.
    -   Generate checksums.
    -   Publish a Draft Release (or Prerelease).
4.  **Publish**: Manually promote the release in GitHub UI.

### Troubleshooting Builds
-   **Linux MUSL**: If build fails linking `openssl-sys`, ensure `default-features = false` is set for `reqwest` in `verdict-core` (we use `rustls` now).
-   **macOS**: Native validation works on GitHub Actions `macos-15` (Intel) and `macos-14` (ARM).

## 6. Known Issues / Roadmap
-   **Judge Configuration**: Currently limited to OpenAI. Need to support generic providers (Anthropic, Bedrock) via uniform trait.
-   **Python SDK**: `pip install verdict` wrapper is planned to ease adoption for Python teams.
-   **VCR**: Interactive recording mode works but needs better UI for "Checking" diffs.
