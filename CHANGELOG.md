# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

## [0.2.0] - 2025-12-20
### Added
- **Redaction**: Added `--redact-prompts` flag to CLI to ensure PII hygiene in artifacts.
- **CI/CD**: GitHub Actions workflow (`verdict.yml`) running smoke tests in deterministic Replay Mode.
- **Metrics**: Added `regex_match` and `regex_not_match` metrics with support for flags (e.g., `i`, `m`).
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
