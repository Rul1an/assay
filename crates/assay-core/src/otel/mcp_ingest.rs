//! MCP-shaped OTLP/JSON ingest scaffold (documentation only, pub(crate))
//!
//! This module exists as a **contract marker** only. It contains no production decoder,
//! no serde models for OTLP payloads, and no runtime ingestion logic.
//!
//! ## Scope (ADR-042 Slice A)
//!
//! - Fixture corpus in `tests/fixtures/otel-mcp-ingest-v0/` (locally generated via official SDK)
//! - Hermetic lock file (`upstream.lock.json`) binding all vendored sources and corpus hashes
//! - Integration tests (`otel_corpus_hermetic.rs`, `otel_corpus_mutation.rs`) with typed validator
//! - Documentation establishing non-production status and provenance claims
//!
//! ## Non-Goals
//!
//! - No unbounded Deserialize models for OTLP structures in production code
//! - No live receiver, decoder, or reducer (Slice B concern)
//! - No CLI subcommand or public API surface
//! - No public schema contract (no live schema exists)
//!
//! ## Provenance
//!
//! Fixtures are generated using:
//! - `@opentelemetry/sdk-trace-node@2.10.0`
//! - `@opentelemetry/exporter-trace-otlp-http@0.221.0`
//! - Deterministic IDs, fixed timestamps, official OTLP HTTP exporter output
//! - **Not external deployment evidence** (self-attestation in sidecar `.meta.json`)
//!
//! ## Lock Enforcement
//!
//! Every element is hashed and locked in `upstream.lock.json`:
//! - SDK: package, version, integrity, resolved URL
//! - Exporter: package, version, integrity, resolved URL
//! - Proto files: 4 vendored `.proto` files from `opentelemetry-proto v1.11.0` with SHA-256
//! - MCP semconv: `semantic-conventions-genai` commit 434c91dc, `docs/gen-ai/mcp.md` with SHA-256
//! - Generator: `package.json`, `package-lock.json`, `generate.js`, `check-runtime.cjs`,
//!   and `.node-version` with SHA-256; exact runtime pair governance (lock `node_version`
//!   and `npm_version`, `package.json` `packageManager` = `npm@<governed>`, package-lock
//!   root `engines.node` = governed Node version)
//! - Corpus: Every fixture with sidecar (provenance, SHA-256, byte count, span kind, MCP method)
//! - Hostile: Locked list of adversarial inputs for Slice B rejection testing
//!
//! Integration tests validate the lock with mutation coverage for the locked contract surface
//! (bit flip, hash tamper, missing file, unlisted file, duplicate lock field,
//! external_deployment true, exact source identity, exact corpus cardinality, exact hostile
//! purpose, exact attribute values, etc.) with typed errors that never include user values.
//!
//! ## Drift Monitoring
//!
//! Non-required workflow `.github/workflows/otel_mcp_fixture_drift.yml` (schedule + dispatch)
//! checks upstream drift for:
//! - SDK and exporter versions (npm registry latest)
//! - All 4 proto files (against opentelemetry-proto v1.11.0 tag)
//! - MCP semconv (against semantic-conventions-genai commit 434c91dc)
//!
//! Drift findings are summary-only and non-required; they do not block required CI.
//! Operational failure (network, API) may fail this informational run.
//!
//! ## Generator
//!
//! Located in `tests/fixtures/otel-mcp-ingest-v0/generator/`:
//! - Standard `package.json` with exact pinned dependencies
//! - `generate.js` uses official OTLP HTTP exporter and deterministic ID generator
//! - Captures official exporter output via ephemeral HTTP server (no hand-serialization)
//! - **Byte-identical** output across runs (fixed timestamps, trace/span IDs)
//!
//! Regenerate:
//! ```bash
//! cd crates/assay-core/tests/fixtures/otel-mcp-ingest-v0/generator
//! npm ci
//! npm run generate
//! ```
//!
//! Verify hashes match lock. CI never runs the generator -- it only validates the locked corpus.
//!
//! ## Vendor
//!
//! - `vendor/opentelemetry-proto-v1.11.0/`: 4 `.proto` files from tag v1.11.0
//!   - `opentelemetry/proto/collector/trace/v1/trace_service.proto`
//!   - `opentelemetry/proto/trace/v1/trace.proto`
//!   - `opentelemetry/proto/resource/v1/resource.proto`
//!   - `opentelemetry/proto/common/v1/common.proto`
//! - `vendor/semantic-conventions-genai-434c91dc/`: MCP semconv markdown
//!
//! ## Corpus
//!
//! Benign fixtures (CLIENT/SERVER span pairs):
//! - `mcp_client_tools_call.json`: SpanKind CLIENT, `mcp.method.name=tools/call`
//! - `mcp_server_tools_call.json`: SpanKind SERVER, `mcp.method.name=tools/call`
//!
//! Each has `.meta.json` sidecar with:
//! - `provenance.generator="locally_generated_official_sdk"`
//! - `provenance.external_deployment=false`
//! - SHA-256 of exact fixture bytes (including trailing newline)
//! - SDK and exporter versions
//!
//! Hostile fixtures (locked inputs for Slice B):
//! - `hostile_deep_nesting.json`: Parser depth limit test
//! - `hostile_oversized_attribute.json`: Size limit test
//! - `hostile_missing_required_fields.json`: Schema validation test
//!
//! ## Future Work (Slice B and beyond)
//!
//! - Test-only or internal bounded OTLP parser for projection to `TraceEvent`
//! - Hostile fixture rejection semantics (depth limits, size limits, schema validation)
//! - Optional CLI integration (`assay evidence inspect-otel-mcp`) if requirements emerge
//!
//! This file intentionally contains no types, functions, or serde models -- it is
//! documentation only.
