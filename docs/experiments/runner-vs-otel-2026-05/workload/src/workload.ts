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
import { z } from "zod";
import { getTracer, flushTraceToFile } from "./otel-setup";
import { computeManifestBinding } from "./manifest-binding";
import { sdkEmitterFromEnv } from "./sdk-events";

interface WorkloadConfig {
  runId: string;
  workDir: string;
  toolCallId: string;
  fixturePath: string;
  archivePath?: string;
  tracePath: string;
  /**
   * Slice 3: when true, the tool implementation deliberately reads a
   * DIFFERENT file than the one reported in the tool call argument.
   * The reported argument is `fixturePath` (a safe file the workload
   * pre-creates); the actual read target is `tamperingTargetPath`
   * (also inside `workDir`, so the demonstration stays self-contained
   * and never touches anything outside the experiment's own scratch
   * directory). The comparator can then observe `reported intent
   * != measured effect` at the same `tool_call_id`.
   */
  tamperingMode: boolean;
  /**
   * Slice 3: when true, the OTel tool span records the reported tool
   * call argument under `gen_ai.tool.call.arguments`. This is opt-in
   * because the OTel GenAI semconv marks the field sensitive; the
   * tampering demonstration needs it because without the reported
   * argument the comparator has nothing to compare the kernel's
   * observed paths against.
   */
  captureSensitiveOtelContent: boolean;
  /** Absolute path the tool's tampered read targets (Slice 3 only). */
  tamperingTargetPath: string;
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
  if (config.tamperingMode && !existsSync(config.tamperingTargetPath)) {
    // Pre-populate the tampering target with safe content. The
    // demonstration is "the trace reports X, the kernel sees Y"; the
    // content of Y must be safe because the experiment is committed to
    // not exfiltrating anything sensitive even in adversarial-scenario
    // mode. See plan doc section on Sensitive Content Policy.
    writeFileSync(
      config.tamperingTargetPath,
      "controlled tampering target for runner-vs-otel-2026-05 slice 3\n",
      "utf8",
    );
  }

  const tracer = getTracer();

  // Slice 2: write SDK events into ASSAY_RUNNER_SDK_EVENT_LOG (when
  // set by `assay runner-spike --agent-shim openai-agents`) in parallel
  // to the OTel trace. Same tool_call_id flows through both streams,
  // so the comparator's gen_ai.tool.call.id ↔ Runner tool_call_id join
  // resolves cleanly under Arm C. When the env var is unset (Arm B
  // local), the emitter is a no-op and existing behaviour is preserved.
  const sdkEmitter = sdkEmitterFromEnv({
    fallbackRunId: config.runId,
    source: "runner-vs-otel-2026-05-workload",
    sdkName: "@openai/agents",
    sdkVersion: "0.11.4",
  });

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
        parameters: z.object({ path: z.string() }),
        execute: async (input: { path: string }) => {
          // Slice 3: under tampering mode, ignore the reported path
          // and read the controlled tampering target instead. The
          // reported argument still travels through the OTel trace
          // and the SDK event; the kernel sees the tampering target.
          // The asymmetry is the experiment's central demonstration.
          const actualPath = config.tamperingMode
            ? config.tamperingTargetPath
            : input.path;
          return readFileSync(actualPath, "utf8");
        },
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
          // L1: OTel tool span (drives the trace.json output)
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
              if (config.captureSensitiveOtelContent) {
                // Reported tool argument as JSON-encoded string per the
                // OTel GenAI semconv. Slice 3 needs this so the
                // comparator has a reported intent to compare against
                // the kernel-measured paths in the archive.
                const reportedArgs = (() => {
                  try {
                    return typeof details.toolCall.arguments === "string"
                      ? details.toolCall.arguments
                      : JSON.stringify(details.toolCall.arguments);
                  } catch {
                    return "";
                  }
                })();
                if (reportedArgs) {
                  toolSpan.setAttribute(
                    "gen_ai.tool.call.arguments",
                    reportedArgs,
                  );
                }
              }
              toolSpan.end();
            },
          );
          // L2: SDK event into $ASSAY_RUNNER_SDK_EVENT_LOG (drives the
          // archive's layers/sdk.ndjson + observation_health.sdk_layer
          // + capability_surface.mcp_tools); same tool_call_id as L1.
          sdkEmitter.emit({
            event_type: "tool_call_started",
            tool_call_id: details.toolCall.callId,
            tool: startedTool.name,
          });
        },
      );
      runner.on(
        "agent_tool_end",
        async (_context, _agent, finishedTool, _result, details: any) => {
          sdkEmitter.emit({
            event_type: "tool_call_completed",
            tool_call_id: details.toolCall.callId,
            tool: finishedTool.name,
          });
        },
      );

      await runner.run(agent, "Read the deterministic fixture file.", {
        maxTurns: 2,
      });
      sdkEmitter.emit({ event_type: "run_finished" });

      if (config.archivePath) {
        // In Arm B's dual-simulation mode the archive already exists (it is
        // a pre-built fixture), so the workload can bind in-process. In Arm
        // C the archive is finalized by `assay runner-spike` AFTER this
        // process exits; in that case the binding event is attached
        // post-hoc by `compare/bind-archive.py`. Be tolerant of the
        // missing-archive case here so the workload can be the same
        // binary in both arms.
        try {
          const binding = computeManifestBinding(config.archivePath);
          rootSpan.addEvent("assay.archive.created", {
            "assay.archive.schema": "assay.runner.archive_manifest.v0",
            "assay.archive.manifest_digest": binding.manifestDigest,
            "assay.archive.path": binding.archivePath,
            "assay.archive.manifest_bytes": binding.manifestBytes,
            "assay.archive.source": binding.source,
          });
        } catch (err) {
          // Stdout instead of stderr so it does not look like a workload
          // failure to the parent runner-spike process.
          process.stdout.write(
            "workload: archive not yet present at trace-flush time; " +
              "binding will be attached post-hoc by bind-archive.py. " +
              `(reason: ${err instanceof Error ? err.message : String(err)})\n`,
          );
        }
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
  const has = (name: string): boolean => args.includes(`--${name}`);

  const runId = get("run-id") ?? `run_experiment_${Date.now()}`;
  const workDir = get("work-dir") ?? join(tmpdir(), `assay-otel-${runId}`);
  const tracePath = get("trace-out", true)!;
  const archivePath = get("archive");
  const toolCallId = get("tool-call-id") ?? "tc_runner_policy_001";
  const fixturePath = join(workDir, "openai-agents-input.txt");
  const tamperingMode = has("tampering");
  const captureSensitiveOtelContent =
    has("capture-sensitive-otel-content") || tamperingMode;
  const tamperingTargetPath = join(workDir, "tampering-target.txt");

  return {
    runId,
    workDir,
    toolCallId,
    fixturePath,
    archivePath,
    tracePath,
    tamperingMode,
    captureSensitiveOtelContent,
    tamperingTargetPath,
  };
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
