#!/usr/bin/env node
/**
 * Fake Codex app-server stdio child. Protocol-shaped replies only. Never a model.
 */
import readline from "node:readline";
import fs from "node:fs";
import path from "node:path";
import { DECIDE_INPUT } from "../../codex_host_proof_validator.mjs";

const RELEASE_DECIDE_TOOL = "assay_policy_decide";

const FAKE_USER_AGENT = "assay-codex-host-proof-fake/1";
const TOOLS = {
  assay_check_args: { name: "assay_check_args", inputSchema: {} },
  assay_check_sequence: { name: "assay_check_sequence", inputSchema: {} },
  assay_policy_decide: { name: "assay_policy_decide", inputSchema: {} },
  assay_check_coverage: { name: "assay_check_coverage", inputSchema: {} },
  assay_explain_trace: { name: "assay_explain_trace", inputSchema: {} },
};

function argValue(flag, fallback) {
  const index = process.argv.indexOf(flag);
  return index >= 0 ? process.argv[index + 1] : fallback;
}

const scenario = argValue("--scenario", "valid");
const ackMarker = argValue("--ack-marker", "");
if (scenario === "close-stdin-then-elicit-exit-0") {
  process.on("SIGTERM", () => {});
}
const projectRoot = argValue("--project-root", process.env.HOME || "/tmp/assay-fake-project");
const threads = new Map();
let threadSeq = 0;

function write(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

function skillEntry() {
  if (scenario === "missing-skill") {
    return { cwd: projectRoot, errors: [], skills: [] };
  }
  return {
    cwd: projectRoot,
    errors: [],
    skills: [
      {
        name: "assay-golden-path",
        description:
          scenario === "credential-shaped-description"
            ? "ghp_0123456789abcdefghijklmnopqrstuvwxyz"
            : "golden path",
        enabled: true,
        path: path.join(projectRoot, ".agents/skills/assay-golden-path/SKILL.md"),
        scope: "repo",
      },
    ],
  };
}

function assayStatus(thread) {
  if (scenario === "clean-missing-binary" && thread.kind === "missing") {
    return {
      name: "assay",
      authStatus: "unsupported",
      resourceTemplates: [],
      resources: [],
      runtimeStatus: "connected",
      tools: TOOLS,
    };
  }
  if (scenario === "clean-invalid-root" && thread.kind === "invalid") {
    return {
      name: "assay",
      authStatus: "unsupported",
      resourceTemplates: [],
      resources: [],
      runtimeStatus: "connected",
      tools: TOOLS,
    };
  }
  if (thread.kind === "missing" || thread.kind === "invalid") {
    return {
      name: "assay",
      authStatus: "unsupported",
      resourceTemplates: [],
      resources: [],
      runtimeStatus: "failed",
      tools: {},
    };
  }
  if (scenario === "missing-tool") {
    const tools = { ...TOOLS };
    delete tools.assay_policy_decide;
    return {
      name: "assay",
      authStatus: "unsupported",
      resourceTemplates: [],
      resources: [],
      runtimeStatus: "connected",
      tools,
    };
  }
  return {
    name: "assay",
    authStatus: "unsupported",
    resourceTemplates: [],
    resources: [],
    runtimeStatus: "connected",
    tools: TOOLS,
  };
}

function threadKind(params) {
  const assay = params?.config?.mcp_servers?.assay;
  const command = assay?.command ?? "";
  const args = assay?.args ?? [];
  if (String(command).includes("missing-assay-mcp-server")) {
    return "missing";
  }
  if (args.some((value) => String(value).includes("missing-policy-root"))) {
    return "invalid";
  }
  return "primary";
}

function handle(message) {
  if (!message || typeof message !== "object") {
    return;
  }
  const { id, method, params, result } = message;
  if (scenario === "exit-1-after-success" && method == null && id === "elicit-1") {
    if (result?.action !== "accept") {
      process.exit(4);
    }
    if (ackMarker) {
      fs.writeFileSync(ackMarker, "accept\n");
    }
    process.exit(1);
  }
  if (method === "initialize") {
    const result = {
      userAgent: FAKE_USER_AGENT,
      platformFamily: "unix",
      platformOs: "macos",
      codexHome: path.join(projectRoot, ".codex-home"),
    };
    if (scenario === "notification-impersonates-response") {
      write({ id, method: "unrelated/notification", result });
      return;
    }
    if (scenario === "result-and-error-response") {
      write({ id, result, error: { code: -32603, message: "result and error" } });
      return;
    }
    if (scenario === "duplicate-initialize-response") {
      write({ id, result });
      write({ id, result });
      return;
    }
    if (scenario === "unknown-response-id") {
      write({ id: 999, result: { unexpected: true } });
      write({ id, result });
      return;
    }
    write({ id, result });
    return;
  }
  if (method === "initialized") {
    if (
      scenario === "strict-initialized-wire" &&
      (!params || typeof params !== "object" || Array.isArray(params) || Object.keys(params).length !== 0)
    ) {
      process.exit(3);
    }
    return;
  }
  if (method === "skills/list") {
    write({ id, result: { data: [skillEntry()] } });
    return;
  }
  if (method === "thread/start") {
    threadSeq += 1;
    const threadId = `thread-${threadSeq}`;
    const cwd = scenario === "wrong-cwd" ? "/not/the/project" : params?.cwd ?? projectRoot;
    threads.set(threadId, { kind: threadKind(params), cwd });
    write({
      id,
      result: {
        cwd,
        approvalPolicy: "on-request",
        approvalsReviewer: "user",
        model: "none",
        modelProvider: "none",
        sandbox: { type: "readOnly" },
        thread: {
          id: threadId,
          cwd,
          cliVersion: "fake",
          createdAt: 0,
          ephemeral: true,
          modelProvider: "none",
          preview: "",
          projectId: null,
          sessionId: "session-1",
          source: "appServer",
          status: { type: "idle" },
          turns: [],
          updatedAt: 0,
        },
      },
    });
    return;
  }
  if (method === "mcpServerStatus/list") {
    const threadId = params?.threadId;
    if (typeof threadId !== "string" || threadId.length === 0 || !threads.has(threadId)) {
      write({
        id,
        error: { code: -32602, message: "unknown or missing threadId for mcpServerStatus/list" },
      });
      return;
    }
    write({ id, result: { data: [assayStatus(threads.get(threadId))] } });
    return;
  }
  if (method === "turn/start") {
    const threadId = params?.threadId;
    if (typeof threadId !== "string" || threadId.length === 0 || !threads.has(threadId)) {
      write({
        id,
        error: { code: -32602, message: "unknown or missing threadId for turn/start" },
      });
      return;
    }
    write({
      id,
      result: {
        turn: { id: "turn-1", items: [], status: "inProgress" },
      },
    });
    if (scenario === "truncated") {
      process.stdout.write('{"method":"item/completed","params":');
      return;
    }
    if (scenario === "unavailable-stream") {
      process.exit(0);
      return;
    }
    if (scenario === "turn-failed") {
      write({
        method: "turn/completed",
        params: {
          threadId,
          turn: { id: "turn-1", items: [], status: "failed" },
        },
      });
      return;
    }
    const elicitParams = () => {
      if (scenario === "foreign-elicit") {
        return {
          serverName: "not-assay",
          threadId,
          turnId: "turn-1",
          message: `approve ${RELEASE_DECIDE_TOOL}`,
          mode: "form",
          requestedSchema: { type: "object", properties: {} },
        };
      }
      if (scenario === "elicit-wrong-thread") {
        return {
          serverName: "assay",
          threadId: "unrelated-thread",
          turnId: "turn-1",
          message: `approve ${RELEASE_DECIDE_TOOL}`,
          mode: "form",
          requestedSchema: { type: "object", properties: {} },
        };
      }
      if (scenario === "elicit-assay-name-only") {
        return { serverName: "assay" };
      }
      if (scenario === "elicit-export-profile") {
        return {
          serverName: "assay",
          threadId,
          turnId: "turn-1",
          message: "Export profile data",
          mode: "form",
          requestedSchema: {
            type: "object",
            properties: { confirm: { type: "boolean" } },
          },
        };
      }
      // The default scenario keeps emitting the legacy prompt so legacy acceptance stays covered
      // by every existing driven run. The canonical scenario emits the prompt the real 0.153.1 host
      // sends, so the widened path this change exists for is exercised end-to-end and not only by a
      // unit assertion.
      return {
        serverName: "assay",
        threadId,
        turnId: "turn-1",
        message:
          scenario === "canonical-elicitation-prompt"
            ? `Allow the assay MCP server to run tool "${RELEASE_DECIDE_TOOL}"?`
            : `approve ${RELEASE_DECIDE_TOOL}`,
        mode: "form",
        requestedSchema: { type: "object", properties: {} },
      };
    };
    if (scenario === "close-stdin-then-elicit-exit-0") {
      fs.closeSync(0);
    }
    // A host chooses its own request ids, and a string id is host text like any other. This
    // scenario uses a secret-bearing one so the absence assertion is made against persisted bytes,
    // while the request/response pairing must still survive.
    const elicitId =
      scenario === "host-rpc-id-leak" || scenario === "host-rpc-id-distinct"
        ? "elicit_/Users/alice/.ssh/id_rsa?sk=RPCID_PROBE"
        : "elicit-1";
    write({
      id: elicitId,
      method: "mcpServer/elicitation/request",
      params: elicitParams(),
    });
    // A second, DIFFERENT host id. Without two distinct ids in one run, injectivity is not
    // observable at all, and a map that collapsed every id to one token would pass unnoticed.
    if (scenario === "host-rpc-id-distinct") {
      write({
        id: "elicit_/Users/bob/.aws/credentials?sk=RPCID_PROBE_TWO",
        method: "mcpServer/elicitation/request",
        params: elicitParams(),
      });
    }
    const tool = scenario === "wrong-tool" ? "assay_check_args" : RELEASE_DECIDE_TOOL;
    const argumentsPayload =
      scenario === "wrong-tool"
        ? { tool: "other" }
        : { ...DECIDE_INPUT };
    const emitTool = () => {
      const completedItem = {
        type: "mcpToolCall",
        id: "call-1",
        server: "assay",
        tool,
        arguments: argumentsPayload,
        status: "completed",
        result: {
          content: [
            { type: "text", text: JSON.stringify({ allowed: true, reason: "Allowed by policy" }) },
          ],
          structuredContent: null,
        },
      };
      // A host that puts data in a KEY NAME leaks exactly as much as one that puts it in a
      // value. This scenario drives such names through the real driver and the real persistence
      // path, so the absence assertion can be made against the bytes written to events.json
      // rather than against an in-process projection.
      // A JSON-RPC method name and a retained item type are host-controlled text on the wire,
      // exactly like a key name. This scenario emits both in secret-bearing form so the absence
      // assertion is made against the persisted bytes.
      if (scenario === "host-identifier-leak") {
        const probe = "/Users/alice/.aws/credentials?sk=IDENTIFIER_PROBE";
        write({ method: `notify_${probe}`, params: {} });
        write({
          method: "item/completed",
          params: {
            completedAtMs: 1,
            threadId,
            turnId: "turn-1",
            item: {
              type: `type_${probe}`,
              id: "item-probe",
              server: "assay",
              tool: "unmapped",
              arguments: { tool: "x", policy: "y" },
              status: "completed",
            },
          },
        });
      }
      if (scenario === "host-key-name-leak") {
        const probe = "/Users/alice/.ssh/id_rsa?token=AKIA_KEY_NAME_PROBE";
        completedItem[probe] = 1;
        completedItem.arguments = { ...argumentsPayload, [probe]: 1 };
        completedItem.result = { ...completedItem.result, [probe]: 1 };
        completedItem.appContext = { connectorId: "connector-1", [probe]: 1 };
        completedItem.error = { message: "boom", [probe]: 1 };
      }
      write({
        method: "item/completed",
        params: {
          completedAtMs: 1,
          threadId,
          turnId: "turn-1",
          item: completedItem,
        },
      });
      if (scenario === "tool-then-failed-turn") {
        write({
          method: "turn/completed",
          params: {
            threadId,
            turn: { id: "turn-1", items: [], status: "failed" },
          },
        });
        return;
      }
      if (scenario === "tool-then-interrupted-turn") {
        write({
          method: "turn/completed",
          params: {
            threadId,
            turn: { id: "turn-1", items: [], status: "interrupted" },
          },
        });
        return;
      }
      write({
        method: "turn/completed",
        params: {
          threadId,
          turn: { id: "turn-1", items: [completedItem], status: "completed" },
        },
      });
    };
    if (scenario === "early-user-then-tool") {
      write({
        method: "item/completed",
        params: {
          completedAtMs: 1,
          threadId,
          turnId: "turn-1",
          item: {
            type: "userMessage",
            id: "um-1",
            content: [{ type: "text", text: "user" }],
          },
        },
      });
      setTimeout(emitTool, 100);
      return;
    }
    if (scenario === "delayed-tool") {
      setTimeout(emitTool, 100);
      return;
    }
    emitTool();
    if (scenario === "close-stdin-then-elicit-exit-0") {
      setImmediate(() => process.exit(0));
    }
    return;
  }
}

const rl = readline.createInterface({ input: process.stdin });
rl.on("line", (line) => {
  if (!line.trim()) {
    return;
  }
  handle(JSON.parse(line));
});
rl.on("close", () => {
  if (scenario === "delayed-tool" || scenario === "early-user-then-tool") {
    process.exit(0);
  }
});
