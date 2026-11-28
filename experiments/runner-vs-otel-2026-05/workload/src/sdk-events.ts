/**
 * Parallel SDK event emitter for the runner-vs-otel-2026-05 experiment.
 *
 * Under Arm C (`assay runner-spike --agent-shim openai-agents -- node
 * workload.js`), the parent `assay` process exposes three env vars that
 * the agent shim is expected to honour:
 *
 *   ASSAY_RUNNER_SDK_EVENT_LOG   path to an NDJSON log file the runner
 *                                folds into the archive's
 *                                layers/sdk.ndjson (and through that
 *                                into observation_health.sdk_layer)
 *   ASSAY_RUNNER_RUN_ID          the run id the parent uses for archive
 *                                identity; the SDK events must carry the
 *                                same id
 *   ASSAY_RUNNER_SDK_EVENT_SCHEMA the schema string events must declare;
 *                                normally `assay.runner.sdk_event.v0`
 *
 * The pre-existing `runner-fixtures/openai-agents/fixture-agent.js`
 * fixture writes into this log; our experiment workload was missing
 * this path, which left the archive's SDK layer empty and made the
 * comparator's `gen_ai.tool.call.id` join report `archive-side-absent`
 * for Arm C. Slice 2 of the runner-vs-otel experiment closes that gap
 * without removing the OTel tracing path (the two streams now run in
 * parallel and share the same `tool_call_id`).
 *
 * Event shape mirrors what assay-runner-schema's `SdkLayerEvent`
 * accepts (see crates/assay-runner-schema/src/sdk_event.rs): schema,
 * run_id, seq, event_type, source, sdk_name?, sdk_version?,
 * tool_call_id?, tool?
 */

import { appendFileSync, existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";

export interface SdkEventCore {
  event_type: string;
  tool_call_id?: string;
  tool?: string;
}

export interface SdkEmitterConfig {
  logPath: string;
  runId: string;
  schema: string;
  source: string;
  sdkName?: string;
  sdkVersion?: string;
}

export interface SdkEmitter {
  emit(event: SdkEventCore): void;
  /** True iff a usable log path was discovered in the environment. */
  readonly active: boolean;
}

/**
 * Construct an emitter from environment variables. If
 * `ASSAY_RUNNER_SDK_EVENT_LOG` is not set (Arm B, local dev), the
 * emitter is a no-op and `active` is false — the workload can call
 * `emit()` unconditionally and Arm B behaviour is preserved.
 */
export function sdkEmitterFromEnv(opts: {
  fallbackRunId: string;
  source: string;
  sdkName?: string;
  sdkVersion?: string;
}): SdkEmitter {
  const logPath = process.env.ASSAY_RUNNER_SDK_EVENT_LOG;
  if (!logPath) {
    return new NoOpSdkEmitter();
  }
  const runId = process.env.ASSAY_RUNNER_RUN_ID || opts.fallbackRunId;
  const schema =
    process.env.ASSAY_RUNNER_SDK_EVENT_SCHEMA || "assay.runner.sdk_event.v0";
  return new FileSdkEmitter({
    logPath,
    runId,
    schema,
    source: opts.source,
    sdkName: opts.sdkName,
    sdkVersion: opts.sdkVersion,
  });
}

class NoOpSdkEmitter implements SdkEmitter {
  public readonly active = false;
  emit(_event: SdkEventCore): void {
    // intentional no-op; Arm B and local dev have no SDK log to write to
  }
}

class FileSdkEmitter implements SdkEmitter {
  public readonly active = true;
  private seq = 0;

  constructor(private readonly cfg: SdkEmitterConfig) {
    // Ensure the parent directory exists; truncate (overwrite) the log
    // on construction so a re-run of the same workload doesn't leave
    // stale events from a previous run.
    const dir = dirname(cfg.logPath);
    if (!existsSync(dir)) {
      mkdirSync(dir, { recursive: true });
    }
    writeFileSync(cfg.logPath, "", "utf8");
  }

  emit(event: SdkEventCore): void {
    const record = {
      schema: this.cfg.schema,
      run_id: this.cfg.runId,
      seq: this.seq,
      source: this.cfg.source,
      sdk_name: this.cfg.sdkName ?? null,
      sdk_version: this.cfg.sdkVersion ?? null,
      ...event,
    };
    appendFileSync(this.cfg.logPath, JSON.stringify(record) + "\n", "utf8");
    this.seq += 1;
  }
}
