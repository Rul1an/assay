# OTLP/JSON MCP Fixture Corpus v0

Pinned to OpenTelemetry SDK trace 1.28.0 (GenAI semconv 1.28.0).

## Honest Fixtures

Generated locally with the official SDK via `scripts/generate_otel_mcp_fixtures.js`
using the OTLP/JSON export format.

- `minimal_chat.json`: Minimal chat operation (gen_ai.operation.name = "chat")
- `tool_execution.json`: Tool execution operation (gen_ai.operation.name = "execute_tool")

Each fixture has a `.meta.json` sidecar with SHA-256 content hash and provenance.

## Hostile Fixtures

Hand-crafted adversarial inputs for tamper and boundary testing:

- `hostile_deep_nesting.json`: Deeply nested kvlistValue to test JSON depth limits
- `hostile_missing_required_fields.json`: Spans missing traceId/spanId/timestamps
- `hostile_oversized_attribute.json`: Attribute value exceeding typical size bounds

## Lock Validation

`upstream.lock.json` pins the SDK dependency closure. The hermetic lock validator
(in `assay_core::otel::mcp_ingest` tests) verifies that fixture provenance matches
the pinned SDK version and rejects tampered fixtures.

## Drift Detection

The non-required scheduled workflow `.github/workflows/otel_mcp_fixture_drift.yml`
periodically checks if the upstream SDK, protobuf definitions, or generator lockfile
have changed. It outputs to GITHUB_STEP_SUMMARY (does not fail or block).
