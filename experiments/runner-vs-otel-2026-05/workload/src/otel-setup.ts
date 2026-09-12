/**
 * OpenTelemetry SDK wiring for the runner-vs-otel-2026-05 experiment.
 *
 * Exports an OTLP/JSON trace to a local file so the experiment is self-
 * contained: no collector, no network, no shared backend. The file is the
 * single trace artifact for the experiment write-up and the input to
 * `compare/compare.py`.
 *
 * SemConv pinning: this experiment targets OpenTelemetry GenAI semantic
 * conventions as fetched on 2026-05-24, plus the Assay-specific
 * `assay.*` namespace defined in
 * docs/experiments/runner-vs-otel-shape-comparison-2026-05.md. Attribute
 * names should be re-checked against
 * https://opentelemetry.io/docs/specs/semconv/gen-ai/ before publication.
 */

import { writeFileSync, mkdirSync, existsSync } from "node:fs";
import { dirname } from "node:path";
import {
  BasicTracerProvider,
  SimpleSpanProcessor,
  InMemorySpanExporter,
  ReadableSpan,
} from "@opentelemetry/sdk-trace-base";
import { resourceFromAttributes } from "@opentelemetry/resources";
import { trace, Tracer } from "@opentelemetry/api";

const SERVICE_NAME = "assay-runner-otel-experiment";

let providerSingleton: BasicTracerProvider | null = null;
let exporterSingleton: InMemorySpanExporter | null = null;

export function getTracer(): Tracer {
  if (providerSingleton === null) {
    exporterSingleton = new InMemorySpanExporter();
    providerSingleton = new BasicTracerProvider({
      resource: resourceFromAttributes({
        "service.name": SERVICE_NAME,
        "service.version": "0.0.0",
      }),
      spanProcessors: [new SimpleSpanProcessor(exporterSingleton)],
    });
    // OTel SDK 2.x: BasicTracerProvider does not auto-register globally.
    // Set it as the global trace provider so `trace.getTracer(...)` works.
    trace.setGlobalTracerProvider(providerSingleton);
  }
  return trace.getTracer("assay-runner-otel-experiment");
}

/**
 * Force-flush the provider and write the in-memory spans to an OTLP/JSON file.
 *
 * Uses a hand-rolled OTLP/JSON serializer (no extra dependency on
 * `@opentelemetry/otlp-transformer`) because the experiment only needs the
 * shape that `compare/compare.py` understands. If the OTLP/JSON spec evolves,
 * the official transformer can be dropped in here.
 */
export async function flushTraceToFile(outPath: string): Promise<void> {
  if (providerSingleton === null || exporterSingleton === null) {
    throw new Error("getTracer() must be called before flushTraceToFile()");
  }
  await providerSingleton.forceFlush();
  const spans = exporterSingleton.getFinishedSpans();
  const doc = spansToOtlpJson(spans);
  mkdirSync(dirname(outPath), { recursive: true });
  writeFileSync(outPath, JSON.stringify(doc, null, 2) + "\n", "utf8");
}

function spansToOtlpJson(spans: ReadableSpan[]): unknown {
  return {
    resourceSpans: [
      {
        resource: {
          attributes: attrsToOtlp({
            "service.name": SERVICE_NAME,
            "service.version": "0.0.0",
          }),
        },
        scopeSpans: [
          {
            scope: { name: "assay-runner-otel-experiment" },
            spans: spans.map((s) => readableSpanToOtlp(s)),
          },
        ],
      },
    ],
  };
}

function readableSpanToOtlp(span: ReadableSpan): unknown {
  return {
    name: span.name,
    spanId: span.spanContext().spanId,
    traceId: span.spanContext().traceId,
    parentSpanId: span.parentSpanContext?.spanId,
    kind: kindToOtlp(span.kind),
    startTimeUnixNano: hrTimeToNs(span.startTime),
    endTimeUnixNano: hrTimeToNs(span.endTime),
    attributes: attrsToOtlp(span.attributes as Record<string, unknown>),
    events: span.events.map((e) => ({
      name: e.name,
      timeUnixNano: hrTimeToNs(e.time),
      attributes: attrsToOtlp((e.attributes ?? {}) as Record<string, unknown>),
    })),
    status: { code: span.status.code },
  };
}

function kindToOtlp(kind: number): string {
  // OTel SpanKind enum -> OTLP SPAN_KIND_* string.
  switch (kind) {
    case 0:
      return "SPAN_KIND_INTERNAL";
    case 1:
      return "SPAN_KIND_SERVER";
    case 2:
      return "SPAN_KIND_CLIENT";
    case 3:
      return "SPAN_KIND_PRODUCER";
    case 4:
      return "SPAN_KIND_CONSUMER";
    default:
      return "SPAN_KIND_UNSPECIFIED";
  }
}

function hrTimeToNs(time: [number, number]): string {
  const [seconds, nanos] = time;
  return (BigInt(seconds) * BigInt(1_000_000_000) + BigInt(nanos)).toString();
}

function attrsToOtlp(attrs: Record<string, unknown>): unknown[] {
  const out: unknown[] = [];
  for (const [key, value] of Object.entries(attrs)) {
    if (value === undefined || value === null) {
      continue;
    }
    out.push({ key, value: valueToOtlp(value) });
  }
  return out;
}

function valueToOtlp(value: unknown): unknown {
  if (typeof value === "string") {
    return { stringValue: value };
  }
  if (typeof value === "number") {
    if (Number.isInteger(value)) {
      return { intValue: String(value) };
    }
    return { doubleValue: value };
  }
  if (typeof value === "boolean") {
    return { boolValue: value };
  }
  if (typeof value === "bigint") {
    return { intValue: value.toString() };
  }
  if (Array.isArray(value)) {
    return {
      arrayValue: {
        values: value.map((v) => valueToOtlp(v)),
      },
    };
  }
  // Fallback: stringify unknown shapes so the export is still valid.
  return { stringValue: JSON.stringify(value) };
}

export function ensureOutDir(path: string): void {
  const dir = dirname(path);
  if (!existsSync(dir)) {
    mkdirSync(dir, { recursive: true });
  }
}
