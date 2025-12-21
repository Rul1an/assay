# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

## [0.2.0] - 2025-12-21

### Added
- **Baseline regression gating**: Compare candidate runs against known-good baselines
- **Relative thresholds**: `max_drop` config to catch score regressions
- **verdict-action improvements**: `baseline` and `export_baseline` inputs
- **Schema versioning**: Strict validation prevents baseline/suite mismatches (Exit 2)

### Changed
- CI mode now defaults to `--strict` when baseline is provided

### Fixed
- Path resolution in verdict-action for monorepo setups
- **Baselines (PR11)**: Detect regressions using `--baseline <file>`.
  - Supports `relative` thresholding logic (e.g. `max_drop: 0.01`).
  - Strict schema versioning and suite mismatch hardening (Exit 2).
- **CI Polish (PR12)**: First-class GitHub Action support.
  - New inputs: `baseline`, `export_baseline`, `upload_exported_baseline`.
  - Robust "Git Show" workflow recommended in docs.
- **Redaction**: Added `--redact-prompts` flag to CLI to ensure PII hygiene in artifacts.
- **CI/CD**: GitHub Actions workflow (`verdict.yml`) running smoke tests in deterministic Replay Mode.
- **Docs**: Comprehensive [User Guide](docs/user-guide.md) and new `init` onboarding.
- **Onboarding**: `verdict init --ci` generates production-ready CI scaffolding.
- **Metrics**: Added `regex_match` (PR5), `json_schema` (PR6), `semantic_similarity` (PR8).
- **LLM-as-a-Judge**: Built-in `Faithfulness` and `Relevance` metrics (OpenAI-based).
- **Config**: Added support for relative file paths in configuration (resolves relative to config file).
- **Strict Mode**: Added `--strict` flag to fail CI on `Warn` or `Flaky` statuses (Exit 1). Default is non-blocking.
- **Reporting**: JUnit reports now mark `Warn`/`Flaky` as passing tests with `<system-out>` logs, improving CI visibility.
- **CLI Refactor**: Unified runner initialization and clarified help strings.
- **Trace Injection**: Run evaluations offline using `--trace-file <path.jsonl>`.
  - Supports strictly deterministic replay of LLM interactions.
  - **Replay Semantics**: Forces `rerun_failures=0` in replay mode. Injects `verdict.replay=true` in metadata.
  - **Hardened Schema**: Enforces Trace Schema v1 (version, type) and unique `request_id`/`prompt`.
- **OpenTelemetry Export**: New `--otel-jsonl` flag for `verdict ci`.
  - Adheres to OTel GenAI Semantic Conventions.
  - Attributes include `gen_ai.system`, `verdict.status`, `verdict.score`.
- **Unit Tests**: Added coverage for `TraceClient` logic.

### Changed
- **Project Structure**: Consolidated codebase to root directory, removing `verdict-v3-mvp-skeleton`.
- **Dependencies**: Added `chrono` for timestamping and `tempfile` for testing.

## [0.1.0] - Skeleton
### Added
- Initial MVP Skeleton.
- Core engine with VCR caching (SQLite).
- Basic JUnit and SARIF reporting.
- CLI scaffolding (`run`, `ci`, `init`, `quarantine`).
