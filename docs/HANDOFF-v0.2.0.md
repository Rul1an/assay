# Handoff: Assay v0.2.0

**Date:** 2025-12-21
**Version:** v0.2.0
**Branch:** `main`

## TL;DR for Maintainers 🚀
*   **Status**: Stable, Hardened, Clean ("0 warnings").
*   **Tag**: `v0.2.0` pushed to `main`.
*   **Key New Feat**: Baselines (Regression Testing) & `assay-action` hardening.
*   **CI**: Green. Uses deterministic replay.
*   **Immediate Action**: Ready to be used via `uses: Rul1an/assay-action@v0.2.0`.

---

## 1. Project Overview
**Assay** is a high-performance, local-first LLM evaluation engine written in Rust. It focuses on deterministic replay, CI/CD integration, and privacy-first design (local traces, PII redaction).

### Key Features
- **Deterministic Replay**: Record LLM interactions once, replay them reliably in CI.
- **Metric Suite**:
  - `must_contain` / `must_not_contain` (substring checks)
  - `regex_match` / `regex_not_match` (regex checks)
  - `json_schema` (structured output validation)
  - `semantic_similarity_to` (embeddings + cosine similarity)
  - `faithfulness` / `relevance` (LLM-as-a-Judge)
- **Baselines**: Detect regressions relative to a known-good baseline using relative thresholds (`max_drop`, `min_floor`).
- **CI/CD Ready**: GitHub Action (`assay-action`), JUnit/SARIF reporting, and strict failure modes.

## 2. Codebase Structure
The project is organized as a Cargo Workspace with three primary crates + an action wrapper:

| Component | Path | Purpose |
| :--- | :--- | :--- |
| **assay-core** | `crates/assay-core` | Engine heart: `Runner`, SQLite storage/cache, providers (LLM/embedder/judge), traces, baseline logic. |
| **assay-metrics** | `crates/assay-metrics` | Metric implementations (regex, schema, semantic similarity, judge metrics). |
| **assay-cli** | `crates/assay-cli` | CLI wiring (`clap`), config loading, runner assembly, reporting outputs. |
| **assay-action** | `assay-action/` | GitHub Action wrapper around the Assay CLI (Marketplace-ready). |

## 3. Key Workflows

### 3.1 Local Development
- **Build**: `cargo build`
- **Test**: `cargo test`
- **Run (Dev)**: `cargo run -- run --config examples/rag-grounding.yaml`
- **Release Build**: `cargo build --release`

### 3.2 Running Evaluations
- **Standard Run**:
  ```bash
  assay run --config assay.yaml
  ```
- **Run with Baseline Comparison**:
  ```bash
  assay run --config assay.yaml --baseline baseline_main.json
  ```
- **Export Baseline**:
  ```bash
  assay ci --config assay.yaml --export-baseline baseline_new.json
  ```

### 3.3 CI/CD Integration
Recommended integration uses the `assay-action`. See `docs/user-guide.md` for the baseline workflow.

Two common baseline storage models:

**A) Baseline in-repo (simplest)**
1. Export baseline on main, commit `baseline.json`.
2. In PR workflow, load `baseline.json` from the repo.

**B) Baseline as artifact**
1. Export baseline on main, upload as artifact.
2. In PR workflow, download artifact and use `baseline: baseline.json`.

**Example for in-repo baseline fetch (PR workflow):**
```bash
git show origin/main:baseline.json > baseline.json
```

## 4. Configuration
Configuration combines `assay.yaml` (tests + settings) and CLI args.
- **Runner settings**: `parallel`, timeouts, config-relative path resolution.
- **Providers**:
    - Replay/trace provider (`--trace-file`) for deterministic CI.
    - Live providers (e.g. OpenAI) for LLM/embedder/judge, requiring `OPENAI_API_KEY`.
- **Privacy**: `--redact-prompts` prevents prompt leakage in outputs/artifacts.

## 5. Recent Changes (v0.2.0)
- **Baselines & relative thresholds**: Export + compare gating with schema hardening.
- **Action hardening**: Baseline flags forwarded; CI DX improved.
- **Cleanup**: Reduced boilerplate; improved docs and examples.

## 6. Known Issues / Roadmap
- **Distribution**: Release automation (building + attaching binaries to GitHub Releases) may require signing/secrets setup.
- **Marketplace**: Action is tagged but Marketplace publishing is manual (if desired).
- **UX**: No interactive TUI; CLI remains the primary interface.

## 7. Operational Status
- **Tests**: All green (`cargo test`)
- **Lints**: Clean (`cargo check`)
- **Dependencies**: Locked and reproducible

**Ready for immediate use.**
