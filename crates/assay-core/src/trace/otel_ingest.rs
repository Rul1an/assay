//! OTLP/MCP Ingest Scaffold (pub(crate) only, no production decoder)
//!
//! This module exists solely as a **contract marker** for future OTLP/JSON MCP trace ingestion.
//! It contains **no serde models, no parsers, and no production code paths**.
//!
//! ## Scope (Slice A)
//!
//! - Fixture corpus in `tests/fixtures/otel-mcp-ingest-v0/` (locally generated via official SDK)
//! - Hermetic lock file binding all vendored sources and corpus hashes
//! - Integration tests with typed, test-only validator (no blanket `allow(dead_code)`)
//! - Documentation establishing non-production status
//!
//! ## Non-Goals
//!
//! - No `OtelSpan` or other unbounded Deserialize models in production code
//! - No `convert_spans_to_episodes` or projection logic (Slice B concern)
//! - No CLI subcommand or public API surface
//! - No assay.dev schema URL (no live schema exists)
//!
//! ## Provenance
//!
//! Fixtures are generated using:
//! - `@opentelemetry/sdk-trace-node@1.28.0`
//! - `@opentelemetry/exporter-trace-otlp-http@0.56.0`
//! - Deterministic IDs, fixed timestamps, official OTLP HTTP exporter output
//! - **Not external deployment evidence** (self-attestation in sidecar metadata)
//!
//! ## Lock Enforcement
//!
//! Every element is hashed and locked:
//! - 4 proto files from `opentelemetry-proto v1.11.0`
//! - MCP semconv from `semantic-conventions-genai` commit 434c91dc
//! - Generator `package.json`, `package-lock.json`, `generate.js`
//! - Every corpus fixture with sidecar (provenance, SHA-256, byte count, span kind, MCP method)
//!
//! Integration tests validate the lock and provide comprehensive mutation coverage (bit flip,
//! truncate, hash tamper, missing file, unlisted file, duplicate lock field, etc.) with typed
//! errors that never include user-supplied values.
//!
//! ## Future Work (Slice B and beyond)
//!
//! - Test-only or internal bounded OTLP parser for projection to `TraceEvent`
//! - Hostile fixture rejection semantics (depth limits, size limits, schema validation)
//! - Optional CLI integration (`assay import otlp`) if requirements emerge
//!
//! This file intentionally contains no types or functions—it is documentation only.
