#!/usr/bin/env node
'use strict';

const fs = require('node:fs');
const path = require('node:path');
const { Agent, Runner, Usage, tool } = require('@openai/agents');

const { name: SDK_NAME, version: SDK_VERSION } = loadSdkMetadata();

function loadSdkMetadata() {
  let directory = path.dirname(require.resolve('@openai/agents'));
  for (;;) {
    const packageJson = path.join(directory, 'package.json');
    if (fs.existsSync(packageJson)) {
      const metadata = JSON.parse(fs.readFileSync(packageJson, 'utf8'));
      if (metadata.name === '@openai/agents') {
        return metadata;
      }
    }

    const parent = path.dirname(directory);
    if (parent === directory) {
      throw new Error('could not locate @openai/agents package metadata');
    }
    directory = parent;
  }
}

function requiredEnv(name) {
  const value = process.env[name];
  if (!value) {
    throw new Error(`${name} must be set`);
  }
  return value;
}

function optionalIntEnv(name, fallback) {
  const value = process.env[name];
  if (!value) return fallback;
  const parsed = Number.parseInt(value, 10);
  if (!Number.isFinite(parsed) || parsed < 0) {
    throw new Error(`${name} must be a non-negative integer`);
  }
  return parsed;
}

function appendEvent(logPath, event) {
  fs.appendFileSync(logPath, `${JSON.stringify(event)}\n`, 'utf8');
}

function makeEmitter({ logPath, runId, schema }) {
  let seq = 0;
  return (event) => {
    appendEvent(logPath, {
      schema,
      run_id: runId,
      seq,
      source: 'openai-agents-fixture',
      sdk_name: SDK_NAME,
      sdk_version: SDK_VERSION,
      ...event,
    });
    seq += 1;
  };
}

async function applySweepPressure(workDir) {
  const kernelEvents = optionalIntEnv('ASSAY_SWEEP_KERNEL_EVENTS', 0);
  if (kernelEvents <= 0) return;
  const concurrency = Math.max(1, optionalIntEnv('ASSAY_SWEEP_CONCURRENCY', 1));
  const payloadBytes = Math.max(1, optionalIntEnv('ASSAY_SWEEP_PAYLOAD_BYTES', 128));
  const payload = 'x'.repeat(payloadBytes);
  const workers = Math.min(concurrency, kernelEvents);
  const sweepDir = path.join(workDir, 'event-rate-sweep');
  fs.mkdirSync(sweepDir, { recursive: true });
  await Promise.all(
    Array.from({ length: workers }, async (_, worker) => {
      for (let index = worker; index < kernelEvents; index += workers) {
        const target = path.join(sweepDir, `worker-${worker}-${index}.txt`);
        fs.writeFileSync(target, payload, 'utf8');
        fs.readFileSync(target, 'utf8');
      }
    }),
  );
}

class DeterministicToolCallModel {
  constructor({ toolCallId, fixturePath }) {
    this.toolCallId = toolCallId;
    this.fixturePath = fixturePath;
  }

  async getResponse(_request) {
    return {
      usage: new Usage({ requests: 1 }),
      responseId: 'assay-runner-openai-agents-fixture-response',
      output: [
        {
          type: 'function_call',
          callId: this.toolCallId,
          name: 'read_file',
          status: 'completed',
          arguments: JSON.stringify({ path: this.fixturePath }),
        },
      ],
    };
  }

  async *getStreamedResponse(_request) {
    throw new Error('streaming is intentionally unsupported by the deterministic fixture');
  }
}

async function main() {
  const workDir = process.argv[2];
  if (!workDir) {
    throw new Error('usage: fixture-agent.js <work-dir>');
  }

  const logPath = requiredEnv('ASSAY_RUNNER_SDK_EVENT_LOG');
  const runId = requiredEnv('ASSAY_RUNNER_RUN_ID');
  const schema = requiredEnv('ASSAY_RUNNER_SDK_EVENT_SCHEMA');
  const toolCallId = process.env.ASSAY_RUNNER_SDK_TOOL_CALL_ID || 'tc_runner_policy_001';

  fs.mkdirSync(workDir, { recursive: true });
  const fixturePath = path.join(workDir, 'openai-agents-input.txt');
  if (!fs.existsSync(fixturePath)) {
    fs.writeFileSync(fixturePath, 'openai agents fixture input\n', 'utf8');
  }
  fs.writeFileSync(logPath, '', 'utf8');

  const emit = makeEmitter({ logPath, runId, schema });
  const readFile = tool({
    name: 'read_file',
    description: 'Read the deterministic runner-spike fixture file.',
    strict: false,
    parameters: {
      type: 'object',
      properties: {
        path: { type: 'string' },
      },
      required: ['path'],
      additionalProperties: false,
    },
    execute: async (input) => fs.readFileSync(input.path, 'utf8'),
  });

  const agent = new Agent({
    name: 'AssayRunnerSpikeFixture',
    instructions: 'Call read_file exactly once.',
    model: new DeterministicToolCallModel({ toolCallId, fixturePath }),
    tools: [readFile],
    toolUseBehavior: { stopAtToolNames: ['read_file'] },
  });

  const runner = new Runner({
    tracingDisabled: true,
    traceIncludeSensitiveData: false,
    toolExecution: { maxFunctionToolConcurrency: 1 },
  });

  // @openai/agents 0.11.x hook names map to assay.runner.sdk_event.v0:
  // agent_tool_start -> tool_call_started, agent_tool_end -> tool_call_completed.
  runner.on('agent_tool_start', (_context, _agent, startedTool, details) => {
    emit({
      event_type: 'tool_call_started',
      tool_call_id: details.toolCall.callId,
      tool: startedTool.name,
    });
  });
  runner.on('agent_tool_end', (_context, _agent, finishedTool, _result, details) => {
    emit({
      event_type: 'tool_call_completed',
      tool_call_id: details.toolCall.callId,
      tool: finishedTool.name,
    });
  });

  try {
    await runner.run(agent, 'Read the deterministic fixture file.', { maxTurns: 2 });
    await applySweepPressure(workDir);
    emit({ event_type: 'run_finished' });
  } catch (error) {
    emit({ event_type: 'run_failed' });
    throw error;
  }
}

main().catch((error) => {
  console.error(error && error.stack ? error.stack : String(error));
  process.exit(1);
});
