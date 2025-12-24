# Changelog

All notable changes to this project will be documented in this file.

## [0.6.0] - 2025-12-23

### Added
- **Async OpenAI Support**: Full `async`/`await` parity implemented in `assay_sdk.async_openai`. Supports `record_chat_completions`, `record_chat_completions_with_tools`, and `record_chat_completions_stream`.
- **Streaming Capture**: New `record_chat_completions_stream` and `record_chat_completions_stream_with_tools` wrappers to support streaming responses while maintaining tool execution capabilities.
- **Redaction Hooks**: `TraceWriter` now supports a `redact_fn` to filter sensitive data (PII, secrets) before writing to disk.
- **Context Injection**: Opt-in `assay_sdk.context` support to allow implicit passing of the `TraceWriter` via `ContextVars`.

### Changed
- **Documentation**: Overhauled `README.md` with a "Golden Quickstart" and advanced usage guides.
- **Examples**: Consolidated all examples into `examples/openai-demo/` with unified sync and async scripts.

### Fixed
- **Trace Consistency**: Ensured `gen_ai.*` schema keys are consistent across sync, async, and streaming modes.

## [0.3.0] - 2024-01-15
- Initial beta release.
