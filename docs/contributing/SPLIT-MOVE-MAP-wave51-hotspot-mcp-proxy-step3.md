# SPLIT MOVE MAP - Wave 51 MCP Proxy Step3

## Intent

Freeze behavior in `crates/assay-core/src/mcp/proxy.rs` before splitting the proxy hotspot. Step 3 intentionally adds characterization coverage only; it is the safety rail for the later mechanical move.

## Moves

| From | To | Notes |
| --- | --- | --- |
| none | none | Step 3 is characterization only. No proxy code moves in this step. |

## Characterized Seams

| Seam | Contract |
| --- | --- |
| JSON-RPC idempotency key extraction | `_meta.tool_call_id` wins over request id; string and numeric request ids map to `req_*`; missing ids generate `gen_*`. |
| Policy reason-code mapping | known legacy policy codes map to stable `P_*` reason codes; unknown codes fail closed to `P_POLICY_DENY`. |
| Tool-definition observation | empty tool names are ignored and do not mutate outbound tool JSON with `tool_identity`. |
| Decision event projection | source, tool, tool_call_id, decision, reason, request id, match metadata, policy digest snapshot, lane, principal, and auth summary stay projected. |

## Step 4 Candidate Moves

| Candidate responsibility | Target shape | Review risk |
| --- | --- | --- |
| server-to-client tools/list enrichment | `mcp/proxy/server.rs` or `mcp/proxy/tools_list.rs` | Preserve stdout passthrough and per-line processing. |
| client-to-server policy loop | `mcp/proxy/client.rs` or `mcp/proxy/policy_loop.rs` | Preserve exactly-one decision event per tool call attempt. |
| idempotency and reason-code helpers | `mcp/proxy/decision.rs` | Keep fallback behavior stable. |
| tool-definition observation/cache update | `mcp/proxy/tool_observation.rs` | Preserve identity and bounded binding atomicity. |
| spawn/facade state | `mcp/proxy/mod.rs` | Keep `McpProxy::spawn` and `McpProxy::run` as the public facade. |

## Reviewer Focus

- Treat this as behavior-freeze, not architecture movement.
- Prefer additive tests over helper extraction in this step.
- The next PR should be allowed to move code only if these contracts stay green.
