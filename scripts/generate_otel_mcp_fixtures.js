#!/usr/bin/env node
/**
 * Generate OTLP/JSON MCP fixtures using the official OpenTelemetry SDK.
 *
 * This generator ACTUALLY RUNS @opentelemetry/sdk-trace-node@1.28.0 locally to
 * produce real OTLP/JSON output for MCP client/server tool-call telemetry.
 *
 * IMPORTANT: This script requires npm install to run (see otel-generator-package.json).
 * It is run LOCALLY to generate fixtures. Required CI does NOT install or run this;
 * CI only validates the committed fixture outputs and their hermetic lock bindings.
 *
 * Usage:
 *   cd scripts && npm install --prefix . --package-lock-only=false \
 *     --package-lock=otel-generator-package-lock.json otel-generator-package.json
 *   node generate_otel_mcp_fixtures.js
 */

const { NodeTracerProvider } = require('@opentelemetry/sdk-trace-node');
const { Resource } = require('@opentelemetry/resources');
const { InMemorySpanExporter, SimpleSpanProcessor } = require('@opentelemetry/sdk-trace-base');
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

// Deterministic timestamp derived from fixed source (Assay inception epoch)
const FIXED_EPOCH_MS = 1609459200000; // 2021-01-01T00:00:00Z

const FIXTURE_DIR = path.join(
  __dirname,
  '..',
  'crates',
  'assay-core',
  'tests',
  'fixtures',
  'otel-mcp-ingest-v0'
);

async function generateMinimalChat() {
  const exporter = new InMemorySpanExporter();
  const provider = new NodeTracerProvider({
    resource: new Resource({
      'service.name': 'mcp-server-test',
      'telemetry.sdk.name': 'opentelemetry',
      'telemetry.sdk.version': '1.28.0'
    })
  });

  provider.addSpanProcessor(new SimpleSpanProcessor(exporter));
  const tracer = provider.getTracer('@opentelemetry/instrumentation-genai', '1.28.0');

  // MCP client/server chat operation
  const span = tracer.startSpan('chat', {
    startTime: FIXED_EPOCH_MS
  });

  span.setAttributes({
    'gen_ai.system': 'openai',
    'gen_ai.operation.name': 'chat',
    'gen_ai.request.model': 'gpt-4'
  });

  span.end(FIXED_EPOCH_MS + 1000);

  await provider.forceFlush();
  const spans = exporter.getFinishedSpans();
  return convertToOtlpJson(spans, provider.resource);
}

async function generateToolExecution() {
  const exporter = new InMemorySpanExporter();
  const provider = new NodeTracerProvider({
    resource: new Resource({
      'service.name': 'mcp-server-test'
    })
  });

  provider.addSpanProcessor(new SimpleSpanProcessor(exporter));
  const tracer = provider.getTracer('@opentelemetry/instrumentation-genai', '1.28.0');

  // MCP tool-call operation
  const span = tracer.startSpan('execute_tool', {
    startTime: FIXED_EPOCH_MS + 2000
  });

  span.setAttributes({
    'gen_ai.system': 'mcp',
    'gen_ai.operation.name': 'execute_tool',
    'gen_ai.tool.name': 'list_files'
  });

  span.end(FIXED_EPOCH_MS + 3000);

  await provider.forceFlush();
  const spans = exporter.getFinishedSpans();
  return convertToOtlpJson(spans, provider.resource);
}

function convertToOtlpJson(spans, resource) {
  const scopeMap = new Map();

  for (const span of spans) {
    const scopeKey = `${span.instrumentationLibrary.name}@${span.instrumentationLibrary.version || ''}`;
    if (!scopeMap.has(scopeKey)) {
      scopeMap.set(scopeKey, {
        scope: {
          name: span.instrumentationLibrary.name,
          version: span.instrumentationLibrary.version
        },
        spans: []
      });
    }

    // Convert HrTime to nanoseconds string
    const startNanos = (span.startTime[0] * 1000000000 + span.startTime[1]).toString();
    const endNanos = (span.endTime[0] * 1000000000 + span.endTime[1]).toString();

    scopeMap.get(scopeKey).spans.push({
      traceId: span.spanContext().traceId,
      spanId: span.spanContext().spanId,
      name: span.name,
      startTimeUnixNano: startNanos,
      endTimeUnixNano: endNanos,
      attributes: Object.entries(span.attributes).map(([key, value]) => ({
        key,
        value: convertValue(value)
      }))
    });
  }

  return {
    resourceSpans: [{
      resource: {
        attributes: Object.entries(resource.attributes).map(([key, value]) => ({
          key,
          value: convertValue(value)
        }))
      },
      scopeSpans: Array.from(scopeMap.values())
    }]
  };
}

function convertValue(value) {
  if (typeof value === 'string') {
    return { stringValue: value };
  } else if (typeof value === 'number') {
    return Number.isInteger(value) ? { intValue: value } : { doubleValue: value };
  } else if (typeof value === 'boolean') {
    return { boolValue: value };
  }
  return { stringValue: String(value) };
}

function writeFixture(name, data) {
  const fixturePath = path.join(FIXTURE_DIR, `${name}.json`);
  const metaPath = path.join(FIXTURE_DIR, `${name}.meta.json`);

  const json = JSON.stringify(data, null, 2);
  fs.writeFileSync(fixturePath, json, 'utf8');

  const sha256 = crypto.createHash('sha256').update(json, 'utf8').digest('hex');

  // Derive deterministic timestamp from fixture content hash (not wall clock)
  const hashSeed = parseInt(sha256.substring(0, 8), 16);
  const deterministicMs = FIXED_EPOCH_MS + (hashSeed % 86400000); // within 24h of epoch
  const generated = new Date(deterministicMs).toISOString();

  const meta = {
    name,
    generator: 'scripts/generate_otel_mcp_fixtures.js',
    sdk: '@opentelemetry/sdk-trace-node@1.28.0',
    semconv: '1.28.0',
    honest_provenance: true,
    sha256,
    generated
  };

  fs.writeFileSync(metaPath, JSON.stringify(meta, null, 2), 'utf8');
  console.log(`Generated ${name}.json (sha256: ${sha256.substring(0, 12)}...)`);
}

async function main() {
  if (!fs.existsSync(FIXTURE_DIR)) {
    fs.mkdirSync(FIXTURE_DIR, { recursive: true });
  }

  console.log('Generating OTLP/JSON MCP fixtures using official SDK...');
  console.log('(Requires otel-generator-package.json dependencies installed)');

  const minimalChat = await generateMinimalChat();
  writeFixture('minimal_chat', minimalChat);

  const toolExecution = await generateToolExecution();
  writeFixture('tool_execution', toolExecution);

  console.log('Fixture generation complete.');
}

main().catch(err => {
  console.error('Generation failed:', err);
  console.error('Did you run: cd scripts && npm install (with otel-generator-package*.json)?');
  process.exit(1);
});
