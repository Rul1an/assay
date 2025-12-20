# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

## [0.2.0] - 2025-12-20
### Added
- **Trace Injection**: Run evaluations offline using `--trace-file <path.jsonl>`.
  - Supports strictly deterministic replay of LLM interactions.
  - Validates uniqueness of prompts to prevent ambiguity.
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
