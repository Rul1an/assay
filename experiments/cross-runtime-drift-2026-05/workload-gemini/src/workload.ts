/**
 * cross-runtime-drift-2026-05 — workload-gemini
 *
 * Implements the workload contract using @google/genai with manual
 * function-calling (automaticFunctionCalling.disable = true). The
 * dispatch loop is OUR code so the wrapper is visible/deterministic
 * and SDK auto-dispatch does not silently add drift.
 *
 * See WORKLOAD_CONTRACT.md for the rules every workload implementation
 * must satisfy.
 */

import {
  mkdirSync,
  readFileSync,
  writeFileSync,
  appendFileSync,
  statSync,
} from "node:fs";
import { resolve, dirname, relative, isAbsolute } from "node:path";
import { sdkEmitterFromEnv, type SdkEmitter } from "./sdk-events";
import {
  GoogleGenAI,
  type Content,
  type FunctionCall,
  type FunctionDeclaration,
  type Part,
} from "@google/genai";

interface WorkloadEnv {
  workDir: string;
  inputPath: string;
  outputPath: string;
  inputContents: string;
  model: string;
  apiKey: string;
}

function loadEnv(): WorkloadEnv {
  const workDir = process.env.WORKLOAD_WORK_DIR;
  if (!workDir) {
    throw new Error("WORKLOAD_WORK_DIR is required");
  }
  const apiKey = process.env.GOOGLE_API_KEY;
  if (!apiKey) {
    throw new Error("GOOGLE_API_KEY is required");
  }
  const absWorkDir = resolve(workDir);
  mkdirSync(absWorkDir, { recursive: true });

  const inputPath = resolve(
    process.env.WORKLOAD_INPUT_PATH ?? `${absWorkDir}/fixture-input.txt`,
  );
  const outputPath = resolve(
    process.env.WORKLOAD_OUTPUT_PATH ?? `${absWorkDir}/fixture-output.txt`,
  );
  const inputContents =
    process.env.WORKLOAD_INPUT_CONTENTS ?? "cross-runtime drift fixture\n";
  // gemini-2.0-flash is no longer available to new users as of 2026-05;
  // the API returns 404 with "Please update your code to use a newer
  // model for the latest features and improvements." Default bumped to
  // gemini-2.5-flash, which is the current generally-available stable
  // model. Override via WORKLOAD_MODEL if you need to pin a specific
  // version (the run-meta.json captures whatever was used so the
  // baseline carries the pin).
  const model = process.env.WORKLOAD_MODEL ?? "gemini-2.5-flash";

  return {
    workDir: absWorkDir,
    inputPath,
    outputPath,
    inputContents,
    model,
    apiKey,
  };
}

/**
 * Contract violation: the model asked for an action the workload contract
 * does not allow (path outside WORK_DIR, wrong path for tool, etc.). The
 * outer try/catch turns this into exit code 2. Distinct from generic
 * Errors which become exit code 1.
 */
class ContractViolationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ContractViolationError";
  }
}

function ensureInsideWorkDir(workDir: string, candidate: string): string {
  const abs = resolve(candidate);
  // Use path.relative() instead of a "/" string-prefix check so the
  // containment test is portable across path separators. A candidate is
  // inside workDir iff relative(workDir, abs) is empty (same path),
  // does not start with "..", and is not absolute (different drive on
  // Windows).
  const rel = relative(workDir, abs);
  const insideOrSame =
    rel === "" || (!rel.startsWith("..") && !isAbsolute(rel));
  if (!insideOrSame) {
    throw new ContractViolationError(
      `Path ${candidate} is outside WORKLOAD_WORK_DIR (${workDir})`,
    );
  }
  return abs;
}

function assertPathEquals(
  toolName: string,
  expected: string,
  candidate: string,
): void {
  if (candidate !== expected) {
    throw new ContractViolationError(
      `${toolName}: path mismatch — expected ${expected}, got ${candidate}`,
    );
  }
}

function appendToolCall(
  toolCallsPath: string,
  seqRef: { value: number },
  tool: string,
  args: Record<string, unknown>,
): void {
  seqRef.value += 1;
  const line = JSON.stringify({ seq: seqRef.value, tool, args }) + "\n";
  appendFileSync(toolCallsPath, line, { encoding: "utf-8" });
}

const readFileDecl: FunctionDeclaration = {
  name: "read_file",
  description: "Read a UTF-8 file from the workload work directory.",
  parametersJsonSchema: {
    type: "object",
    properties: {
      path: {
        type: "string",
        description: "Absolute path to the file to read.",
      },
    },
    required: ["path"],
  },
};

const writeFileDecl: FunctionDeclaration = {
  name: "write_file",
  description: "Write UTF-8 contents to a file inside the work directory.",
  parametersJsonSchema: {
    type: "object",
    properties: {
      path: {
        type: "string",
        description: "Absolute path to the file to write.",
      },
      contents: {
        type: "string",
        description: "UTF-8 contents to write.",
      },
    },
    required: ["path", "contents"],
  },
};

const SYSTEM_INSTRUCTION =
  "You are a deterministic agent. Use the provided tools to do exactly " +
  "what the user asked. Do not paraphrase the task. Do not add " +
  "commentary. Reply with the literal word `DONE` when the work is " +
  "complete.";

const MAX_LOOP_ITERATIONS = 6;

interface DispatchOutcome {
  exitCode: number;
  modelReply: string;
}

async function runManualLoop(
  ai: GoogleGenAI,
  env: WorkloadEnv,
  toolCallsPath: string,
  seq: { value: number },
  sdk: SdkEmitter,
): Promise<DispatchOutcome> {
  const prompt =
    `Read the file at \`${env.inputPath}\`, uppercase its contents, then ` +
    `write the result to \`${env.outputPath}\`. Call \`read_file\` first ` +
    `and \`write_file\` second. Do not call any other tool. When done, ` +
    `reply with the single word \`DONE\`.`;

  const contents: Content[] = [
    { role: "user", parts: [{ text: prompt }] },
  ];

  for (let iter = 0; iter < MAX_LOOP_ITERATIONS; iter += 1) {
    const response = await ai.models.generateContent({
      model: env.model,
      contents,
      config: {
        temperature: 0,
        systemInstruction: SYSTEM_INSTRUCTION,
        tools: [
          { functionDeclarations: [readFileDecl, writeFileDecl] },
        ],
        automaticFunctionCalling: { disable: true },
      },
    });

    const calls: FunctionCall[] | undefined = response.functionCalls;

    if (!calls || calls.length === 0) {
      const text = String(response.text ?? "").trim();
      if (/^DONE[.!]?$/i.test(text)) {
        return { exitCode: 0, modelReply: text };
      }
      return { exitCode: 3, modelReply: text };
    }

    // Echo the model's function-call turn into history so the SDK keeps
    // the conversation coherent. parts must be { functionCall: ... }.
    contents.push({
      role: "model",
      parts: calls.map((c) => ({ functionCall: c }) as Part),
    });

    const responseParts: Part[] = [];
    for (const call of calls) {
      const name = call.name ?? "";
      const args = (call.args ?? {}) as Record<string, unknown>;

      // Synthesize a tool_call_id from the call's own .id when the SDK
      // provides it, otherwise from seq — both work for the drift
      // comparator's tool_invocation_order dimension.
      const callId =
        (typeof call.id === "string" && call.id) ||
        `tc_gemini_${seq.value + 1}`;

      if (name === "read_file") {
        const path = ensureInsideWorkDir(
          env.workDir,
          String(args.path ?? ""),
        );
        assertPathEquals("read_file", env.inputPath, path);
        sdk.emit({
          event_type: "tool_call_started",
          tool_call_id: callId,
          tool: "read_file",
        });
        appendToolCall(toolCallsPath, seq, "read_file", { path });
        const data = readFileSync(path, "utf-8");
        sdk.emit({
          event_type: "tool_call_completed",
          tool_call_id: callId,
          tool: "read_file",
        });
        responseParts.push({
          functionResponse: {
            name,
            response: { output: data },
          },
        });
      } else if (name === "write_file") {
        const path = ensureInsideWorkDir(
          env.workDir,
          String(args.path ?? ""),
        );
        assertPathEquals("write_file", env.outputPath, path);
        const fileContents = String(args.contents ?? "");
        sdk.emit({
          event_type: "tool_call_started",
          tool_call_id: callId,
          tool: "write_file",
        });
        appendToolCall(toolCallsPath, seq, "write_file", {
          path,
          contents: fileContents,
        });
        mkdirSync(dirname(path), { recursive: true });
        writeFileSync(path, fileContents, { encoding: "utf-8" });
        sdk.emit({
          event_type: "tool_call_completed",
          tool_call_id: callId,
          tool: "write_file",
        });
        responseParts.push({
          functionResponse: {
            name,
            response: { output: "ok" },
          },
        });
      } else {
        // Unregistered tool — contract violation. Surface upstream.
        return {
          exitCode: 2,
          modelReply: `model invoked unknown tool: ${name}`,
        };
      }
    }

    contents.push({ role: "user", parts: responseParts });
  }

  return {
    exitCode: 3,
    modelReply: `loop exceeded ${MAX_LOOP_ITERATIONS} iterations without DONE`,
  };
}

async function main(): Promise<number> {
  const env = loadEnv();
  const toolCallsPath = `${env.workDir}/tool-calls.ndjson`;
  const metaPath = `${env.workDir}/run-meta.json`;

  writeFileSync(toolCallsPath, "", { encoding: "utf-8" });

  mkdirSync(dirname(env.inputPath), { recursive: true });
  writeFileSync(env.inputPath, env.inputContents, { encoding: "utf-8" });

  const seq = { value: 0 };
  const ai = new GoogleGenAI({ apiKey: env.apiKey });

  // SDK-event emitter. Active when assay runner-spike has set
  // ASSAY_RUNNER_SDK_EVENT_LOG; no-op for local dev. The emitter's
  // constructor truncates the log path, which is what runner-spike
  // requires in order to ingest layers/sdk.ndjson — without this the
  // runner exits with "failed to read runner-spike SDK event log ...
  // No such file or directory" even when the workload itself ran fine.
  let geminiSdkVersion = "unknown";
  try {
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    geminiSdkVersion = (require("@google/genai/package.json") as {
      version: string;
    }).version;
  } catch {
    // best-effort
  }
  const sdk = sdkEmitterFromEnv({
    fallbackRunId: process.env.ASSAY_RUNNER_RUN_ID ?? `local_${Date.now()}`,
    source: "gemini-genai",
    sdkName: "@google/genai",
    sdkVersion: geminiSdkVersion,
  });

  const startedAt = new Date().toISOString();
  let outcome: DispatchOutcome = { exitCode: 1, modelReply: "" };
  try {
    outcome = await runManualLoop(ai, env, toolCallsPath, seq, sdk);
  } catch (err) {
    process.stderr.write(`workload-gemini error: ${(err as Error).message}\n`);
    if (err instanceof ContractViolationError) {
      outcome = { exitCode: 2, modelReply: (err as Error).message };
    } else {
      outcome = { exitCode: 1, modelReply: "" };
    }
  }
  const endedAt = new Date().toISOString();

  if (outcome.exitCode === 0) {
    try {
      const st = statSync(env.outputPath);
      if (!st.isFile() || st.size === 0) {
        outcome.exitCode = 2;
      }
    } catch {
      outcome.exitCode = 2;
    }
  }

  sdk.emit({
    event_type: outcome.exitCode === 0 ? "run_finished" : "run_failed",
  });

  const meta = {
    runtime: "gemini-genai",
    model: env.model,
    sdk_version: geminiSdkVersion,
    started_at: startedAt,
    ended_at: endedAt,
    exit_code: outcome.exitCode,
    model_reply: outcome.modelReply,
  };
  writeFileSync(metaPath, JSON.stringify(meta, null, 2) + "\n", {
    encoding: "utf-8",
  });

  return outcome.exitCode;
}

main()
  .then((code) => process.exit(code))
  .catch((err) => {
    process.stderr.write(`workload-gemini fatal: ${(err as Error).stack ?? err}\n`);
    process.exit(1);
  });
