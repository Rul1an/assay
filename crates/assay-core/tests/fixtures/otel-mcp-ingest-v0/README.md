# OTLP MCP Ingest Fixtures v0

Locally generated OpenTelemetry test fixtures using the official SDK and OTLP HTTP exporter.
These are **not external deployment evidence** -- they exist solely for internal MCP semantic
convention testing in Assay.

## Purpose

- **Test-only input corpus**: Locked reference fixtures for future OTLP/JSON MCP ingest work
- **No production decoder**: `assay-core` contains no serde models or parsers for OTLP (Slice A scope)
- **Hermetic validator**: Integration tests use a typed, test-only validator with strict lock checking
- **Future work**: Slice B+ will add `assay evidence inspect-otel-mcp` for semantic validation

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

Hostile fixtures are **locked inputs only** -- Slice B will define rejection semantics.

## Lock File

`upstream.lock.json` binds every corpus element:
- SDK: `@opentelemetry/sdk-trace-node@2.10.0`
- Exporter: `@opentelemetry/exporter-trace-otlp-http@0.221.0`
- Proto files: 4 vendored `.proto` files from `opentelemetry-proto v1.11.0` with SHA-256
- MCP semconv: `semantic-conventions-genai` commit 434c91dc, `docs/gen-ai/mcp.md` with SHA-256
- Generator: `package.json`, `package-lock.json`, `generate.js`, `check-runtime.cjs` with SHA-256
- Runtime pair: `node_version` (22.16.0) and `npm_version` (10.9.2) in lock
- Corpus: Every fixture with sidecar, hash, byte count, span kind, MCP method

## Generator

Located in `generator/`:
- `.node-version` governs the exact Node runtime (`22.16.0`)
- `package.json` `packageManager` field governs the exact npm version (`npm@10.9.2`)
- `check-runtime.cjs` is the preinstall guard: rejects missing/malformed/mismatched Node
  or npm (via `npm_config_user_agent`), does not echo attacker-controlled values
- `generate.js` reads `.node-version` and refuses to run on a different Node version
- Captures official exporter output via ephemeral HTTP server
- **Byte-identical** output across runs on the exact governed pair (fixed timestamps, trace/span IDs)

The `packageManager` field in `package.json` is advisory metadata -- npm itself does not
reject a mismatch. Enforcement is provided by `check-runtime.cjs` (preinstall hook) and
by the validator's `PackageJsonPackageManagerMismatch` check. The package-lock does NOT
carry `packageManager`; it only carries `engines.node` which the validator also checks.

Regenerate:
```bash
cd generator
node --version  # must be exactly 22.16.0
npm --version   # must be exactly 10.9.2
npm ci
npm run generate
```

Verify hashes match lock. CI never runs the generator -- it only validates the locked corpus.

## Vendor

- `vendor/opentelemetry-proto-v1.11.0/`: 4 `.proto` files from tag v1.11.0
- `vendor/semantic-conventions-genai-434c91dc/`: MCP semconv markdown

## Validation

Integration tests (`crates/assay-core/tests/otel_*.rs`) use a hermetic validator that:
1. Parses the lock file and validates schema/provenance markers
2. Validates every vendored file, generator file, and corpus fixture hash
3. Validates exact upstream source identities, repositories, tags/commits, and file paths
4. Validates benign fixture semantics: exact span name, span kind, exact attribute values
5. Validates all sidecar semantics: schema version, fixture name, generator, timestamps
6. Validates exact runtime pair: `package.json` `packageManager` must equal `npm@<governed>`
7. Fails with typed errors (no user values in messages) if any check fails
8. Provides mutation tests for the locked contract surface (byte flip, duplicate fields/paths, absolute paths, exact identity/cardinality/purpose/attribute-value/packageManager checks)

Retained fields checked: span name, span kind, mcp.method.name, gen_ai.operation.name,
gen_ai.tool.name, jsonrpc.request.id, mcp.protocol.version, sidecar provenance, timestamps,
and file hashes. Test-only parser validates fixture structure without introducing unbounded
production parsers into `assay-core`.
