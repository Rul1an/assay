#!/usr/bin/env node

/**
 * Deterministic OTLP/JSON MCP fixture generator
 *
 * Generates locally-produced test fixtures using the official OpenTelemetry SDK
 * and OTLP HTTP exporter. These are NOT external deployment evidence.
 *
 * Key properties:
 * - Deterministic: Fixed trace/span IDs, timestamps
 * - Official: Uses real @opentelemetry/exporter-trace-otlp-http
 * - MCP semconv: Attributes from semantic-conventions-genai MCP spec
 * - Byte-identical: Reproducible output across runs
 */

import { Resource } from '@opentelemetry/resources';
import { ATTR_SERVICE_NAME } from '@opentelemetry/semantic-conventions';
import { NodeTracerProvider, InMemorySpanExporter } from '@opentelemetry/sdk-trace-node';
import { BatchSpanProcessor, SimpleSpanProcessor } from '@opentelemetry/sdk-trace-base';
import { OTLPTraceExporter } from '@opentelemetry/exporter-trace-otlp-http';
import { SpanKind, SpanStatusCode, context } from '@opentelemetry/api';
import * as http from 'http';
import * as fs from 'fs/promises';
import * as path from 'path';
import { fileURLToPath } from 'url';
import { createHash } from 'crypto';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

/**
 * Deterministic ID generator for reproducible fixtures
 */
class DeterministicIdGenerator {
  constructor(seed) {
    this.seed = seed;
    this.counter = 0;
  }

  generateTraceId() {
    // 32 hex chars (16 bytes)
    const input = `trace-${this.seed}-${this.counter++}`;
    const hash = createHash('sha256').update(input).digest('hex');
    return hash.substring(0, 32);
  }

  generateSpanId() {
    // 16 hex chars (8 bytes)
    const input = `span-${this.seed}-${this.counter++}`;
    const hash = createHash('sha256').update(input).digest('hex');
    return hash.substring(0, 16);
  }
}

/**
 * Ephemeral HTTP server to capture OTLP/JSON output
 */
class OTLPCaptureServer {
  constructor() {
    this.receivedData = [];
    this.server = null;
    this.port = null;
  }

  async start() {
    return new Promise((resolve, reject) => {
      this.server = http.createServer((req, res) => {
        if (req.method === 'POST' && req.url === '/v1/traces') {
          let body = '';
          req.on('data', chunk => { body += chunk; });
          req.on('end', () => {
            try {
              this.receivedData.push(JSON.parse(body));
              res.writeHead(200, { 'Content-Type': 'application/json' });
              res.end(JSON.stringify({ partialSuccess: {} }));
            } catch (err) {
              res.writeHead(400);
              res.end();
            }
          });
        } else {
          res.writeHead(404);
          res.end();
        }
      });

      this.server.listen(0, '127.0.0.1', () => {
        this.port = this.server.address().port;
        resolve(this.port);
      });

      this.server.on('error', reject);
    });
  }

  async stop() {
    return new Promise((resolve) => {
      if (this.server) {
        this.server.close(() => resolve());
      } else {
        resolve();
      }
    });
  }

  getUrl() {
    return `http://127.0.0.1:${this.port}/v1/traces`;
  }

  getData() {
    return this.receivedData;
  }
}

/**
 * Create tracer provider with deterministic ID generation
 */
function createTracerProvider(idGenerator, exporterUrl) {
  const resource = Resource.default().merge(
    new Resource({
      [ATTR_SERVICE_NAME]: 'mcp-test-service',
    })
  );

  const provider = new NodeTracerProvider({
    resource,
    idGenerator,
  });

  const exporter = new OTLPTraceExporter({
    url: exporterUrl,
    headers: {},
    concurrencyLimit: 1,
  });

  // Use SimpleSpanProcessor for immediate, deterministic export
  provider.addSpanProcessor(new SimpleSpanProcessor(exporter));

  return provider;
}

/**
 * Generate MCP client tools/call span (CLIENT)
 */
function generateMcpClientToolsCall(tracer, traceId, spanId, parentSpanId) {
  const span = tracer.startSpan(
    'mcp.client.tools.call',
    {
      kind: SpanKind.CLIENT,
      startTime: 1722518400000, // 2024-08-01T12:00:00Z in ms
    },
    context.active()
  );

  // Override IDs for determinism (internal SDK hack for testing)
  span._spanContext.traceId = traceId;
  span._spanContext.spanId = spanId;
  if (parentSpanId) {
    span._spanContext.parentSpanId = parentSpanId;
  }

  // MCP semantic conventions (from semantic-conventions-genai)
  span.setAttribute('mcp.method.name', 'tools/call');
  span.setAttribute('mcp.tool.name', 'read_file');
  span.setAttribute('mcp.client.version', '1.0.0');
  span.setAttribute('mcp.protocol.version', '2024-11-05');

  // Arguments and result
  span.setAttribute('mcp.tool.args', JSON.stringify({ path: '/etc/hosts' }));
  span.setAttribute('mcp.tool.result', JSON.stringify({ content: '127.0.0.1 localhost' }));

  span.setStatus({ code: SpanStatusCode.OK });
  span.end(1722518400500); // 500ms duration

  return span;
}

/**
 * Generate MCP server tools/call span (SERVER)
 */
function generateMcpServerToolsCall(tracer, traceId, spanId, parentSpanId) {
  const span = tracer.startSpan(
    'mcp.server.tools.call',
    {
      kind: SpanKind.SERVER,
      startTime: 1722518400100, // 100ms after client start
    },
    context.active()
  );

  // Override IDs
  span._spanContext.traceId = traceId;
  span._spanContext.spanId = spanId;
  if (parentSpanId) {
    span._spanContext.parentSpanId = parentSpanId;
  }

  // MCP server-side attributes
  span.setAttribute('mcp.method.name', 'tools/call');
  span.setAttribute('mcp.tool.name', 'read_file');
  span.setAttribute('mcp.server.version', '1.0.0');
  span.setAttribute('mcp.protocol.version', '2024-11-05');

  span.setAttribute('mcp.tool.args', JSON.stringify({ path: '/etc/hosts' }));
  span.setAttribute('mcp.tool.result', JSON.stringify({ content: '127.0.0.1 localhost' }));

  span.setStatus({ code: SpanStatusCode.OK });
  span.end(1722518400400); // 300ms duration

  return span;
}

/**
 * Write fixture with sidecar metadata
 */
async function writeFixture(name, data, outputDir) {
  const fixtureData = {
    resourceSpans: data.resourceSpans || [],
  };

  const content = JSON.stringify(fixtureData, null, 2) + '\n';
  const fixturePath = path.join(outputDir, `${name}.json`);
  await fs.writeFile(fixturePath, content, 'utf-8');

  const hash = createHash('sha256').update(content, 'utf-8').digest('hex');

  const sidecar = {
    schema_version: '1',
    fixture_name: name,
    provenance: {
      generator: 'locally_generated_official_sdk',
      external_deployment: false,
      sdk_version: '1.28.0',
      exporter_version: '0.56.0',
    },
    content_sha256: hash,
    byte_count: Buffer.byteLength(content, 'utf-8'),
    generated_at: '2026-08-01T00:00:00Z',
  };

  const sidecarPath = path.join(outputDir, `${name}.meta.json`);
  await fs.writeFile(sidecarPath, JSON.stringify(sidecar, null, 2) + '\n', 'utf-8');

  console.log(`✓ ${name}.json (${hash.substring(0, 8)}..., ${sidecar.byte_count} bytes)`);
}

/**
 * Main generation flow
 */
async function main() {
  const outputDir = path.resolve(__dirname, '..');

  console.log('Starting OTLP/JSON MCP fixture generation...\n');

  const server = new OTLPCaptureServer();
  await server.start();
  console.log(`Capture server listening on ${server.getUrl()}\n`);

  try {
    const idGen = new DeterministicIdGenerator('mcp-fixtures-v1');
    const provider = createTracerProvider(idGen, server.getUrl());
    const tracer = provider.getTracer('mcp-fixture-generator', '1.0.0');

    // Generate mcp_client_tools_call
    const clientTraceId = idGen.generateTraceId();
    const clientSpanId = idGen.generateSpanId();
    generateMcpClientToolsCall(tracer, clientTraceId, clientSpanId, null);

    await provider.forceFlush();
    await new Promise(resolve => setTimeout(resolve, 100));

    if (server.getData().length > 0) {
      await writeFixture('mcp_client_tools_call', server.getData()[0], outputDir);
    }

    // Reset for server fixture
    server.receivedData = [];
    const idGen2 = new DeterministicIdGenerator('mcp-server-v1');
    const provider2 = createTracerProvider(idGen2, server.getUrl());
    const tracer2 = provider2.getTracer('mcp-fixture-generator', '1.0.0');

    const serverTraceId = idGen2.generateTraceId();
    const serverSpanId = idGen2.generateSpanId();
    generateMcpServerToolsCall(tracer2, serverTraceId, serverSpanId, null);

    await provider2.forceFlush();
    await new Promise(resolve => setTimeout(resolve, 100));

    if (server.getData().length > 0) {
      await writeFixture('mcp_server_tools_call', server.getData()[0], outputDir);
    }

    await provider.shutdown();
    await provider2.shutdown();

    console.log('\n✓ Generation complete');
  } finally {
    await server.stop();
  }
}

main().catch(err => {
  console.error('Generation failed:', err);
  process.exit(1);
});
