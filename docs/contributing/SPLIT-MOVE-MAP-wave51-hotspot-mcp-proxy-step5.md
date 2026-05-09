# SPLIT MOVE MAP - Wave 51 MCP Proxy Step5

## Intent

Move one threaded loop at a time. Step 5 moves the server-to-client child stdout loop into `proxy/server.rs` while keeping the thread spawn and join in `McpProxy::run`.

## Moves

| From | To | Notes |
| --- | --- | --- |
| server-to-client `BufReader` loop | `proxy/server.rs::run_server_to_client` | Preserves line-by-line child stdout passthrough. |
| tools/list JSON response parsing | `proxy/server.rs::run_server_to_client` | Preserves enrichment only when `result.tools` is an array. |
| identity cache insertion | `proxy/server.rs::run_server_to_client` | Preserves name-keyed runtime identity cache behavior. |
| tool-definition cache insertion | `proxy/server.rs::run_server_to_client` | Preserves name-keyed bounded binding cache behavior. |
| stdout lock/write/flush for server output | `proxy/server.rs::run_server_to_client` | Preserves serialized writes to shared stdout. |

## Data Flow After Step 5

1. `proxy.rs::McpProxy::run` opens child stdin/stdout, creates shared stdout, and spawns both proxy threads.
2. The server-to-client thread delegates to `proxy::server::run_server_to_client`.
3. `proxy::server::run_server_to_client` reads child stdout, enriches tools/list responses via `proxy::tools::observe_tool_definition`, updates caches, then writes the processed line to stdout.
4. The client-to-server policy loop remains in `proxy.rs` and continues to use the same identity/tool-definition caches.

## Reviewer Focus

- This should be a mechanical loop move, not a behavior rewrite.
- The shared stdout lock remains the same synchronization point.
- The client-to-server policy path is intentionally not split here.
- `proxy/server.rs` should not know policy decisions or audit semantics.
