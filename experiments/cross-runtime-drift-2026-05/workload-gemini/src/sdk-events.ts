/**
 * Parallel SDK event emitter for the cross-runtime-drift-2026-05 experiment.
 *
 * In live Runner captures, the workflow invokes `assay runner-spike` with
 * `--sdk-event-log <path>`. The parent `assay` process then exposes three
 * env vars that each runtime workload is expected to honour:
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
 * These events give each runtime archive a non-empty SDK layer for the drift
 * comparator's SDK-tool and invocation-order dimensions. The same helper is
 * used by the OpenAI and Gemini workload implementations so both arms produce
 * the same event shape.
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
 * `ASSAY_RUNNER_SDK_EVENT_LOG` is not set (local dev / no runner-spike), the
 * emitter is a no-op and `active` is false — the workload can call
 * `emit()` unconditionally and local behaviour is preserved.
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
    // intentional no-op; local dev has no SDK log to write to
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
