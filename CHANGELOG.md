# Changelog

All notable changes to this project will be documented in this file.

## [v0.8.0] - 2025-12-27

### Security & Hardening
*   **Zero Conf Trust**: `assay migrate` ensures all policies are inlined, removing reliance on external mutable policy files.
*   **Cache Key V2**: Cache keys now include trace fingerprints, ensuring invalidation when trace inputs change (ADR-005).
*   **Trace Security**: Enforced V2 Schema for trace ingest.
*   **Strict Mode**: `assay run --strict` now forces `rerun_failures=0` to prevent "retry-until-max-attempts" pattern.

### Features
*   **Auto-Migration**: `assay migrate` automatically upgrades `v0` configs to `v1`.
    *   Inlines external `policy: file.yaml` references.
    *   Converts list-based sequences to Rule DSL.
*   **Sequence DSL**: New `rules` based syntax for `sequence_valid` metric.
    *   `require: tool_name` (order-independent)
    *   `before: { first: A, then: B }`
    *   `blocklist: pattern`
*   **Legacy Compat**: `MCP_CONFIG_LEGACY=1` environment variable to bypass v1 enforcement.
*   **Versioning**: Configs now require explicit `configVersion: 1`.

### Internals
*   **Golden Harness**: E2E regression testing for CLI output stability.
*   **Python SDK Cleanup**: Repository no longer retains `__pycache__` artifacts.
## [v0.5.0] - 2025-12-24

### Features
*   **MCP Integration**: Full support for Model Context Protocol.
    *   **Import**: `assay import --format mcp-inspector --init` to ingest logs and auto-scaffold metrics.
    *   **Verify**: `assay run --replay-strict` ensures deterministic tool execution.
    *   **Metrics**: `args_valid` (JSON Schema) and `sequence_valid` (Strict Ordering).
*   **Hardening**:
    *   `--no-cache` alias for better DX.
    *   `sequence_valid` shows precise diff index on failure.
    *   Robust handling of orphaned tool calls and complex nested arguments.

## [v0.4.0] - 2025-12-23

### Features
*   **SOTA TUI (Agent Demo 2)**: A high-fidelity, `rich`-based terminal UI demonstration.
    *   Features real-time step streaming, live dashboard metrics, and attack simulation scenarios.
    *   Reference implementation for building premium agent interfaces.
*   **CI Regression Gate**: End-to-end example (`examples/ci-regression-gate`) of preventing regression in CI.
    *   Safe: `cargo build --release` from source in workflows (no binary download dependency).
*   **Code Sweep (Senior Polish)**:
    *   **Zero Clippy Warnings**: Strict linting compliance.
    *   **Refactored StoreStats**: Improved type safety in `assay-core`.
    *   **Optimizations**: `f64::clamp` usage, efficient `is_some_and` patterns.

## [v0.3.4] - 2025-12-23

### Features
*   **Adoption Hardening**: A suite of features to improve stability and support.
*   **Assay Doctor**: New command `assay doctor` to generate support bundles (health check, stats, diagnostics).
*   **Assay Validate**: New command `assay validate` for preflight checks of config, traces, and baselines.
*   **Action Hardening**:
    *   **Fork Support**: `sarif: auto` automatically skips Sarif uploads on fork PRs to prevent permission errors.
    *   **Cache Splitting**: Distinct cache keys for DB (`.eval`) and Runtime (`~/.assay`) to prevent cache confusion.
    *   **Monorepo**: New `workdir` input for robust path resolution in nested projects.
*   **Diagnostics**: Standardized error codes (e.g., `E_TRACE_MISS`, `E_BASE_MISMATCH`) with actionable "Fast fix" suggestions.
*   **Troubleshooting Guide**: A new "cookbook" for fixing the top-10 failure modes in CI.

## [0.3.3] - 2025-12-22

### Added
- **Calibration**: New `assay calibrate` command to analyze score distributions and recommend thresholds.
- **Hygiene Report**: New `assay baseline report` command to identify flaky tests, drift, and high failure rates.
- **Strict Replay**: Added `--replay-strict` flag to fail CI with exit code 2 if network calls are attempted (deterministic guardrail).

### Changed
- **Metrics**: `Store::open` fix and performance improvements for result aggregation.
- **Reporting**: Aggregation logic now includes "all attempts" source tracking for P90 scores.

## [0.2.1] - 2025-12-21

### Added
- **Automated Release Workflow**: `v*` tags now trigger builds for Linux (x86_64) and macOS (x86_64, aarch64) with checksums.
- **assay-action**:
  - `baseline` and `export_baseline` inputs for first-class regression gating.
  - Robust artifact upload logic (no more missing file errors).
- **Hardened Validation**: `baseline` checks for `schema_version` and `suite` mismatch now exit with code 2 ("Config Error").
- **UX**: Added warnings for configuration tweaks (fingerprint mismatch) to avoid noisy regressions.

## [0.2.0] - 2025-12-21

### Added
- **Baseline regression gating**: Compare candidate runs against known-good baselines
- **Relative thresholds**: `max_drop` config to catch score regressions
- **assay-action improvements**: `baseline` and `export_baseline` inputs
- **Schema versioning**: Strict validation prevents baseline/suite mismatches (Exit 2)

### Changed
- CI mode now defaults to `--strict` when baseline is provided

### Fixed
- Path resolution in assay-action for monorepo setups
- **Baselines (PR11)**: Detect regressions using `--baseline <file>`.
  - Supports `relative` thresholding logic (e.g. `max_drop: 0.01`).
  - Strict schema versioning and suite mismatch hardening (Exit 2).
- **CI Polish (PR12)**: First-class GitHub Action support.
  - New inputs: `baseline`, `export_baseline`, `upload_exported_baseline`.
  - Robust "Git Show" workflow recommended in docs.
- **Redaction**: Added `--redact-prompts` flag to CLI to ensure PII hygiene in artifacts.
- **CI/CD**: GitHub Actions workflow (`assay.yml`) running smoke tests in deterministic Replay Mode.
- **Docs**: Comprehensive [User Guide](docs/user-guide.md) and new `init` onboarding.
- **Onboarding**: `assay init --ci` generates production-ready CI scaffolding.
- **Metrics**: Added `regex_match` (PR5), `json_schema` (PR6), `semantic_similarity` (PR8).
- **LLM-as-a-Judge**: Built-in `Faithfulness` and `Relevance` metrics (OpenAI-based).
- **Config**: Added support for relative file paths in configuration (resolves relative to config file).
- **Strict Mode**: Added `--strict` flag to fail CI on `Warn` or `Flaky` statuses (Exit 1). Default is non-blocking.
- **Reporting**: JUnit reports now mark `Warn`/`Flaky` as passing tests with `<system-out>` logs, improving CI visibility.
- **CLI Refactor**: Unified runner initialization and clarified help strings.
- **Trace Injection**: Run evaluations offline using `--trace-file <path.jsonl>`.
  - Supports strictly deterministic replay of LLM interactions.
  - **Replay Semantics**: Forces `rerun_failures=0` in replay mode. Injects `assay.replay=true` in metadata.
  - **Hardened Schema**: Enforces Trace Schema v1 (version, type) and unique `request_id`/`prompt`.
- **OpenTelemetry Export**: New `--otel-jsonl` flag for `assay ci`.
  - Adheres to OTel GenAI Semantic Conventions.
  - Attributes include `gen_ai.system`, `assay.status`, `assay.score`.
- **Unit Tests**: Added coverage for `TraceClient` logic.

### Changed
- **Project Structure**: Consolidated codebase to root directory, removing `assay-v3-mvp-skeleton`.
- **Dependencies**: Added `chrono` for timestamping and `tempfile` for testing.

## [0.1.0] - Skeleton
### Added
- Initial MVP Skeleton.
- Core engine with VCR caching (SQLite).
- Basic JUnit and SARIF reporting.
- CLI scaffolding (`run`, `ci`, `init`, `quarantine`).
