# SPLIT MOVE MAP - Wave 51 MCP Proxy Step4

## Intent

Make the first mechanical MCP proxy split without touching the stdio threading loop. This moves pure helpers and their characterization tests into private proxy submodules while preserving `McpProxy` as the public facade.

## Moves

| From | To | Notes |
| --- | --- | --- |
| `McpProxy::handle_allow` | `proxy/decisions.rs::handle_allow` | Preserves audit allow logging and verbose allow message behavior. |
| `McpProxy::extract_tool_call_id` | `proxy/decisions.rs::extract_tool_call_id` | Preserves `_meta.tool_call_id`, request id fallback, and generated id fallback. |
| `McpProxy::map_policy_code` | `proxy/decisions.rs::map_policy_code` | Preserves legacy `E_*` to `P_*` mapping and fail-closed unknown-code mapping. |
| `McpProxy::emit_decision` | `proxy/decisions.rs::emit_decision` | Preserves decision event field projection, policy snapshot projection, obligations, and tool-definition binding projection. |
| `McpProxy::observe_tool_definition` | `proxy/tools.rs::observe_tool_definition` | Preserves tools/list identity computation, bounded tool-definition binding, and outbound `tool_identity` augmentation. |
| `ToolDefinitionObservation` | `proxy/tools.rs::ToolDefinitionObservation` | Keeps observation shape private to the proxy implementation. |
| Step 3 helper tests | `proxy/decisions.rs` / `proxy/tools.rs` | Keeps behavior contracts adjacent to moved code. |

## Data Flow After Step 4

1. `proxy.rs::McpProxy::run` still owns both stdio threads and child lifecycle.
2. The server-to-client loop delegates each tools/list item to `proxy::tools::observe_tool_definition`.
3. The client-to-server loop delegates idempotency, allow logging, reason-code mapping, and decision event projection to `proxy::decisions`.
4. The existing identity and tool-definition caches stay in `McpProxy` for now.

## Reviewer Focus

- This PR should read as mostly moved code plus call-site rewiring.
- No JSON-RPC parse/forward behavior should change.
- No decision-event field should disappear or change meaning.
- No public proxy API should be introduced from the new submodules.
