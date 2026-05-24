/**
 * cross-runtime-drift-2026-05 — workload-openai
 *
 * Implements the workload contract using @openai/agents (real OpenAI API).
 * Mode: standard agent loop (the SDK orchestrates dispatch). This is the
 * runtime under measurement on the OpenAI arm.
 *
 * See WORKLOAD_CONTRACT.md for the rules every workload implementation
 * must satisfy.
 */

import { mkdirSync, readFileSync, writeFileSync, appendFileSync, statSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { Agent, Runner, tool } from "@openai/agents";
import { z } from "zod";

interface WorkloadEnv {
  workDir: string;
  inputPath: string;
  outputPath: string;
  inputContents: string;
  model: string;
}

function loadEnv(): WorkloadEnv {
  const workDir = process.env.WORKLOAD_WORK_DIR;
  if (!workDir) {
    throw new Error("WORKLOAD_WORK_DIR is required");
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
  const model = process.env.WORKLOAD_MODEL ?? "gpt-4o-mini";

  return { workDir: absWorkDir, inputPath, outputPath, inputContents, model };
}

function ensureInsideWorkDir(workDir: string, candidate: string): string {
  const abs = resolve(candidate);
  if (!abs.startsWith(workDir + "/") && abs !== workDir) {
    throw new Error(
      `Path ${candidate} is outside WORKLOAD_WORK_DIR (${workDir})`,
    );
  }
  return abs;
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

async function main(): Promise<number> {
  const env = loadEnv();
  const toolCallsPath = `${env.workDir}/tool-calls.ndjson`;
  const metaPath = `${env.workDir}/run-meta.json`;

  // Reset per-run artefacts. Re-running into the same WORK_DIR must be
  // idempotent at the contract level.
  writeFileSync(toolCallsPath, "", { encoding: "utf-8" });

  // Seed the input fixture. The contract requires the workload to do this
  // itself so the contract-checker can verify against a known input.
  mkdirSync(dirname(env.inputPath), { recursive: true });
  writeFileSync(env.inputPath, env.inputContents, { encoding: "utf-8" });

  const seq = { value: 0 };

  const readFileTool = tool({
    name: "read_file",
    description: "Read a UTF-8 file from the workload work directory.",
    parameters: z.object({
      path: z.string().describe("Absolute path to the file to read."),
    }),
    execute: async ({ path }) => {
      const abs = ensureInsideWorkDir(env.workDir, path);
      appendToolCall(toolCallsPath, seq, "read_file", { path: abs });
      return readFileSync(abs, "utf-8");
    },
  });

  const writeFileTool = tool({
    name: "write_file",
    description: "Write UTF-8 contents to a file inside the work directory.",
    parameters: z.object({
      path: z.string().describe("Absolute path to the file to write."),
      contents: z.string().describe("UTF-8 contents to write."),
    }),
    execute: async ({ path, contents }) => {
      const abs = ensureInsideWorkDir(env.workDir, path);
      appendToolCall(toolCallsPath, seq, "write_file", {
        path: abs,
        contents,
      });
      mkdirSync(dirname(abs), { recursive: true });
      writeFileSync(abs, contents, { encoding: "utf-8" });
      return "ok";
    },
  });

  const agent = new Agent({
    name: "cross-runtime-drift-openai",
    instructions:
      "You are a deterministic agent. Use the provided tools to do exactly " +
      "what the user asked. Do not paraphrase the task. Do not add " +
      "commentary. Reply with the literal word `DONE` when the work is " +
      "complete.",
    model: env.model,
    tools: [readFileTool, writeFileTool],
    modelSettings: {
      temperature: 0,
    },
  });

  const startedAt = new Date().toISOString();
  const runner = new Runner();
  const prompt =
    `Read the file at \`${env.inputPath}\`, uppercase its contents, then ` +
    `write the result to \`${env.outputPath}\`. Call \`read_file\` first ` +
    `and \`write_file\` second. Do not call any other tool. When done, ` +
    `reply with the single word \`DONE\`.`;

  let exitCode = 0;
  let modelReply = "";
  try {
    const result = await runner.run(agent, prompt);
    modelReply = String(result.finalOutput ?? "").trim();
    if (!/^DONE[.!]?$/i.test(modelReply)) {
      exitCode = 3;
    }
  } catch (err) {
    process.stderr.write(`workload-openai error: ${(err as Error).message}\n`);
    exitCode = 1;
  }
  const endedAt = new Date().toISOString();

  // Self-check: if exit_code is still 0, also confirm the agent at least
  // produced the output file. If it didn't, demote to contract-violation 2.
  if (exitCode === 0) {
    try {
      const st = statSync(env.outputPath);
      if (!st.isFile() || st.size === 0) {
        exitCode = 2;
      }
    } catch {
      exitCode = 2;
    }
  }

  const sdkVersion: string = (() => {
    try {
      // eslint-disable-next-line @typescript-eslint/no-require-imports
      const pkg = require("@openai/agents/package.json") as { version: string };
      return pkg.version;
    } catch {
      return "unknown";
    }
  })();

  const meta = {
    runtime: "openai-agents",
    model: env.model,
    sdk_version: sdkVersion,
    started_at: startedAt,
    ended_at: endedAt,
    exit_code: exitCode,
    model_reply: modelReply,
  };
  writeFileSync(metaPath, JSON.stringify(meta, null, 2) + "\n", {
    encoding: "utf-8",
  });

  return exitCode;
}

main()
  .then((code) => process.exit(code))
  .catch((err) => {
    process.stderr.write(`workload-openai fatal: ${(err as Error).stack ?? err}\n`);
    process.exit(1);
  });
