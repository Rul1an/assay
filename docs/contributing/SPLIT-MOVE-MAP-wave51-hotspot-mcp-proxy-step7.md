# SPLIT MOVE MAP - Wave 51 MCP Proxy Step7

## Intent

After Step 6 moved the full client-to-server loop into `proxy/client.rs`, Step 7 narrows that file by extracting policy-decision branch mechanics. The loop remains responsible for reading, parsing, evaluating policy, and forwarding. The new branch module owns the behavior that differs across allow, allow-with-warning, deny, and dry-run deny.

## Moves

| From | To | Notes |
| --- | --- | --- |
| `PolicyDecision::Allow` handling | `proxy/client/branches.rs::handle_allow_branch` | Preserves allow audit logging and always-emit allow decision event for tool calls. |
| `PolicyDecision::AllowWithWarning` handling | `proxy/client/branches.rs::handle_allow_with_warning_branch` | Preserves warning log shape, audit entry, allow decision event, and non-double-logged allow continuation. |
| `PolicyDecision::Deny` handling | `proxy/client/branches.rs::handle_deny_branch` | Preserves deny/would-deny audit entry, mapped reason code, decision event, dry-run forwarding, and blocking deny response. |
| forwarding vs blocking decision | `proxy/client/branches.rs::BranchOutcome` | Makes the loop branch-neutral: forward unless the branch returns `Blocked`. |
| branch inputs | `proxy/client/branches.rs::PolicyBranchContext` | Passes existing request, cached tool params, metadata, binding, config, emitter, event source, and audit log without broadening visibility. |

## Data Flow After Step 7

1. `proxy.rs::McpProxy::run` still spawns and joins the proxy threads.
2. `proxy/client.rs::run_client_to_server` reads stdin lines, parses JSON-RPC, looks up caches, evaluates policy, and builds a `PolicyBranchContext`.
3. `proxy/client/branches.rs::handle_policy_decision` handles allow, warning, deny, and dry-run deny outcomes.
4. `proxy/client.rs` forwards the original line unless the branch outcome is `Blocked`.
5. Deny response writing remains injected by closure so tests can capture the response without running the stdio proxy loop.

## Reviewer Focus

- This should be a mechanical branch split, not a policy rewrite.
- `client.rs` should not regain direct `PolicyDecision` branch arms.
- `branches.rs` should not learn about stdin, child stdin, server stdout parsing, or tools/list enrichment.
- Dry-run deny must continue to emit `Decision::Allow` and forward the original request.
- Non-dry-run deny must continue to emit `Decision::Deny`, write a deny response, and skip forwarding.
