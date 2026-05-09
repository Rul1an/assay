# SPLIT MOVE MAP - Wave 51 MCP Proxy Step6

## Intent

Move the second threaded loop after Step 5 moved server output handling. Step 6 moves the client-to-server stdin/policy/forwarding loop into `proxy/client.rs` while keeping the thread spawn and join in `McpProxy::run`.

## Moves

| From | To | Notes |
| --- | --- | --- |
| client-to-server `stdin.lock` loop | `proxy/client.rs::run_client_to_server` | Preserves line-by-line stdin read and child stdin forwarding. |
| JSON-RPC request parsing | `proxy/client.rs::run_client_to_server` | Preserves best-effort parse and suspicious unparsable JSON warning. |
| identity/tool-definition cache reads | `proxy/client.rs::run_client_to_server` | Preserves name-keyed lookup for tool calls. |
| policy evaluation and state | `proxy/client.rs::run_client_to_server` | Preserves `PolicyState` ownership inside the client loop. |
| allow/warning/deny policy branches | `proxy/client.rs::run_client_to_server` | Preserves audit logging, decision emission, dry-run behavior, and deny response handling. |
| child stdin write/flush | `proxy/client.rs::run_client_to_server` | Preserves forwarding for allowed, warning, dry-run deny, non-tool, and unparsable requests. |

## Data Flow After Step 6

1. `proxy.rs::McpProxy::run` opens child stdin/stdout, creates shared stdout, initializes the decision emitter, then spawns both proxy threads.
2. The server-to-client thread delegates to `proxy::server::run_server_to_client`.
3. The client-to-server thread delegates to `proxy::client::run_client_to_server`.
4. `proxy::client::run_client_to_server` owns the policy path and uses helpers from `proxy::decisions`.
5. `proxy.rs` remains the public facade and process/thread orchestrator.

## Reviewer Focus

- This should be a mechanical loop move, not a policy rewrite.
- `proxy/client.rs` is allowed to know policy/audit/decision details; `proxy/server.rs` is not.
- The deny path must still skip forwarding unless `dry_run` is enabled.
- The suspicious unparsable JSON warning must remain forwarding-only.
