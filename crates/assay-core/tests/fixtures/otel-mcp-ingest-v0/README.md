# OTLP MCP Ingest Fixtures v0

Locally generated OpenTelemetry test fixtures using the official SDK and OTLP HTTP exporter.
These are **not external deployment evidence**—they exist solely for internal MCP semantic
convention testing in Assay.

## Purpose

- **Test-only input corpus**: Validates OTLP/JSON ingestion for MCP-instrumented traces
- **No production decoder**: `assay-core` contains no serde models or parsers for OTLP
- **Hermetic validator**: Integration tests use a typed, test-only validator with strict lock checking

## Fixtures

### Benign (Corpus)

- `mcp_client_tools_call.json`: CLIENT span for `mcp.method.name=tools/call`
- `mcp_server_tools_call.json`: SERVER span for `mcp.method.name=tools/call`

Each fixture has a `.meta.json` sidecar with:
- Provenance: `locally_generated_official_sdk`, `external_deployment=false`
- SHA-256 of exact fixture bytes (including trailing newline)
- SDK and exporter versions

### Hostile

- `hostile_deep_nesting.json`: Parser depth limit test
- `hostile_oversized_attribute.json`: Size limit test
- `hostile_missing_required_fields.json`: Schema validation test

Hostile fixtures are **locked inputs only**—Slice B will define rejection semantics.

## Lock File

`upstream.lock.json` binds every corpus element:
- SDK: `@opentelemetry/sdk-trace-node@1.28.0`
- Exporter: `@opentelemetry/exporter-trace-otlp-http@0.56.0`
- Proto files: 4 vendored `.proto` files from `opentelemetry-proto v1.11.0` with SHA-256
- MCP semconv: `semantic-conventions-genai` commit 434c91dc, `docs/gen-ai/mcp.md` with SHA-256
- Generator: `package.json`, `package-lock.json`, `generate.js` with SHA-256
- Corpus: Every fixture with sidecar, hash, byte count, span kind, MCP method

## Generator

Located in `generator/`:
- Standard `package.json` with exact pinned dependencies
- `generate.js` uses official OTLP HTTP exporter and deterministic IDs
- Captures official exporter output via ephemeral HTTP server
- **Byte-identical** output across runs (fixed timestamps, trace/span IDs)

Regenerate:
```bash
cd generator
npm ci
npm run generate
```

Verify hashes match lock. CI never runs the generator—it only validates the locked corpus.

## Vendor

- `vendor/opentelemetry-proto-v1.11.0/`: 4 `.proto` files from tag v1.11.0
- `vendor/semantic-conventions-genai-434c91dc/`: MCP semconv markdown

## Validation

Integration tests (`crates/assay-core/tests/otel_*.rs`) use a hermetic validator that:
1. Parses the lock file
2. Validates every vendored file, generator file, and corpus fixture hash
3. Fails with typed errors (no user values in messages) if any hash mismatches
4. Provides mutation tests (bit flip, truncate, hash tamper, missing file, unlisted file, etc.)

No field is ignored. No blanket `allow(dead_code)` on test-only types.
