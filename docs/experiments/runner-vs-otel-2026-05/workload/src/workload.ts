/**
 * Workload entrypoint for the runner-vs-otel-2026-05 experiment.
 *
 * Wraps the existing deterministic OpenAI Agents fixture (used by
 * `assay runner-spike` Phase 1 gates) with OpenTelemetry tracing that
 * follows the OTel GenAI semantic conventions plus the experiment's own
 * `assay.*` namespace.
 *
 * Modes:
 *  - Arm B (trace only): the wrapper drives the agent in-process and
 *    produces one OTLP trace per run. No Runner archive is created.
 *  - Arm C (dual capture): the wrapper is invoked under
 *    `assay runner-spike run` so that the Runner archive and the trace are
 *    produced from the same execution. The archive path is then fed back
 *    to attach the manifest-digest binding event to the root span.
 *
 * This file intentionally re-implements the small DeterministicToolCallModel
 * shape inline so the workload can run on macOS / Linux / Windows for Arm B
 * without depending on the original CommonJS fixture script. The behaviour
 * mirrors `runner-fixtures/openai-agents/fixture-agent.js`.
 */

import { writeFileSync, mkdirSync, readFileSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { tmpdir } from "node:os";
import { SpanStatusCode } from "@opentelemetry/api";
import { Agent, Runner, Usage, tool } from "@openai/agents";
import { getTracer, flushTraceToFile } from "./otel-setup";
import { computeManifestBinding } from "./manifest-binding";

interface WorkloadConfig {
  runId: string;
  workDir: string;
  toolCallId: string;
  fixturePath: string;
  archivePath?: string;
  tracePath: string;
}

class DeterministicToolCallModel {
  constructor(
    private readonly toolCallId: string,
    private readonly fixturePath: string,
  ) {}

  async getResponse(_request: unknown): Promise<unknown> {
    return {
      usage: new Usage({ requests: 1 }),
      responseId: "assay-runner-otel-experiment-response",
      output: [
        {
          type: "function_call",
          callId: this.toolCallId,
          name: "read_file",
          status: "completed",
          arguments: JSON.stringify({ path: this.fixturePath }),
        },
      ],
    };
  }

  async *getStreamedResponse(_request: unknown): AsyncIterable<unknown> {
    throw new Error(
      "streaming is intentionally unsupported by the deterministic fixture",
    );
  }
}

export async function runWorkload(config: WorkloadConfig): Promise<void> {
  mkdirSync(config.workDir, { recursive: true });
  if (!existsSync(config.fixturePath)) {
    writeFileSync(
      config.fixturePath,
      "openai agents otel experiment fixture input\n",
      "utf8",
    );
  }

  const tracer = getTracer();

  await tracer.startActiveSpan("assay.runner.measured_run", async (rootSpan) => {
    rootSpan.setAttributes({
      "assay.run.id": config.runId,
      "assay.measurement.boundary": "linux_ebpf_cgroup_v2",
      "gen_ai.provider.name": "openai",
      "gen_ai.operation.name": "create_agent",
    });

    try {
      const readFileTool = tool({
        name: "read_file",
        description: "Read the deterministic experiment fixture file.",
        strict: false,
        parameters: {
          type: "object",
          properties: { path: { type: "string" } },
          required: ["path"],
          additionalProperties: false,
        },
        execute: async (input: { path: string }) =>
          readFileSync(input.path, "utf8"),
      });

      const agent = new Agent({
        name: "AssayRunnerOtelExperiment",
        instructions: "Call read_file exactly once.",
        model: new DeterministicToolCallModel(
          config.toolCallId,
          config.fixturePath,
        ) as unknown as Agent["model"],
        tools: [readFileTool],
        toolUseBehavior: { stopAtToolNames: ["read_file"] },
      });

      const runner = new Runner({
        tracingDisabled: true,
        traceIncludeSensitiveData: false,
        toolExecution: { maxFunctionToolConcurrency: 1 },
      });

      runner.on(
        "agent_tool_start",
        async (_context, _agent, startedTool, details: any) => {
          await tracer.startActiveSpan(
            `execute_tool ${startedTool.name}`,
            async (toolSpan) => {
              toolSpan.setAttributes({
                "assay.run.id": config.runId,
                "gen_ai.operation.name": "execute_tool",
                "gen_ai.tool.name": startedTool.name,
                "gen_ai.tool.type": "function",
                "gen_ai.tool.call.id": details.toolCall.callId,
              });
              toolSpan.end();
            },
          );
        },
      );

      await runner.run(agent, "Read the deterministic fixture file.", {
        maxTurns: 2,
      });

      if (config.archivePath) {
        const binding = computeManifestBinding(config.archivePath);
        rootSpan.addEvent("assay.archive.created", {
          "assay.archive.schema": "assay.runner.archive_manifest.v0",
          "assay.archive.manifest_digest": binding.manifestDigest,
          "assay.archive.path": binding.archivePath,
          "assay.archive.manifest_bytes": binding.manifestBytes,
          "assay.archive.source": binding.source,
        });
      }

      rootSpan.setStatus({ code: SpanStatusCode.OK });
    } catch (error) {
      rootSpan.recordException(
        error instanceof Error ? error : new Error(String(error)),
      );
      rootSpan.setStatus({ code: SpanStatusCode.ERROR });
      throw error;
    } finally {
      rootSpan.end();
    }
  });

  await flushTraceToFile(config.tracePath);
}

function parseArgs(): WorkloadConfig {
  const args = process.argv.slice(2);
  const get = (name: string, required = false): string | undefined => {
    const idx = args.indexOf(`--${name}`);
    if (idx >= 0 && idx + 1 < args.length) return args[idx + 1];
    if (required) throw new Error(`--${name} is required`);
    return undefined;
  };

  const runId = get("run-id") ?? `run_experiment_${Date.now()}`;
  const workDir = get("work-dir") ?? join(tmpdir(), `assay-otel-${runId}`);
  const tracePath = get("trace-out", true)!;
  const archivePath = get("archive");
  const toolCallId = get("tool-call-id") ?? "tc_runner_policy_001";
  const fixturePath = join(workDir, "openai-agents-input.txt");

  return { runId, workDir, toolCallId, fixturePath, archivePath, tracePath };
}

if (require.main === module) {
  const config = parseArgs();
  runWorkload(config)
    .then(() => {
      process.stdout.write(
        JSON.stringify(
          {
            run_id: config.runId,
            trace_out: config.tracePath,
            archive: config.archivePath ?? null,
          },
          null,
          2,
        ) + "\n",
      );
    })
    .catch((error) => {
      console.error(error?.stack ?? String(error));
      process.exit(1);
    });
}
