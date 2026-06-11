# Privileged-action evidence

This is the index for how Assay records and reviews privileged in-application actions an agent takes
through MCP tool calls. Kernel and network enforcement see that an agent connected to a host; they do
not see that, through a tool call, it added a deploy key to a repository or a member to a workspace.
That is the gap this set of records covers.

The pieces compose into one chain:

```
observed tool call
  -> classified privileged action            (tool-decision surface)
  -> declared-vs-observed diff                (declared tool surface)
  -> was the credential scope appropriate?    (credential-scope)
  -> how strong is the side-effect evidence?  (side-effect ladder)
  -> did the tool manifest drift?             (manifest-drift, coarse gate)
```

## Record types

| Record | What it carries | Reference |
|--------|-----------------|-----------|
| `assay.tool_decision_surface.v0` | per-call: server, classified action + projected target, decision, response, redaction | [tool-decision-surface.md](tool-decision-surface.md) |
| `assay.declared_tool_surface.v0` | declared/allowed privileged actions, for observed-vs-declared review | [declared-tool-surface.md](declared-tool-surface.md) |
| `action.required_scope` (+ declared credentials) | the scope an action requires vs the alias's declared scope | [credential-scope.md](credential-scope.md) |
| `assay.provider_audit_record.v0` | an imported provider audit entry, bound to an observed call by digest recompute | [side-effect-receipt.md](side-effect-receipt.md) |
| `assay.mcp_manifest_observed.v0` | an observed MCP tool manifest as canonical digests, for coarse drift review against a declared baseline | [mcp-manifest-drift.md](mcp-manifest-drift.md) |

## Evidence ladder (side effects)

A tool returning success is the provider's assertion, not proof. Side-effect evidence is ranked, not
asserted as truth:

```
asserted            the tool response asserted success
observed_confirmed  a later same-run read tool-call binds the same target
audit_record_bound  an imported provider audit record's binding recomputes from committed bytes
                    AND binds this call
audit_record_verified   reserved: the imported record's signature/issuer is independently verified
```

A consumer may require a minimum level for a privileged action. Ranking the evidence is not the same
as proving the provider-side fact occurred.

## Credential-scope semantics

`action.required_scope` is Assay's static, deterministic claim of what an action needs. It is compared
against the scopes an operator declares for the credential alias the call used, through a small
scope-coverage lattice (coverage, not set membership). Outcomes: sufficient, overbroad (covers via a
broader/admin scope), insufficient, or unknown. Declared scopes are operator metadata, not
provider-verified grants; there is no token introspection.

## Non-claims (the boundaries that hold across all of the above)

- a classified tool call is observed, not proof the provider performed or persisted the action;
- `audit_record_bound` proves a binding recompute over committed bytes, not provider truth and not
  the authenticity of the imported record beyond any signature it carries;
- `observed_confirmed` is same-run readback evidence, not provider verification;
- credential overbreadth is a recommendation signal, not a population claim and not a provider grant;
- manifest drift is canonical-digest evidence, not maliciousness evidence;
- raw secrets, tokens, and key material are never stored; sensitive identifiers are hashed under
  per-field domains; an unknown or incomplete observation is inconclusive, never read as clean.

## Status: shipped, experiment-only, parked

**Shipped (releasable):** the tool-decision surface and rule-based classifiers; the
declared-vs-observed gate; credential-scope evidence; the side-effect receipt spec and the consumer
that ranks the side-effect ladder (`asserted` / `observed_confirmed` / `audit_record_bound`); and the
MCP tool-manifest **coarse** drift path — a producer that builds `assay.mcp_manifest_observed.v0` from
observed tool definitions and a consumer that reviews a supplied artifact against a declared
manifest-digest baseline, resolving a mismatch to `pending_tool_manifest_review`; and (assay v3.23.0)
the **MCP upstream manifest-observation proxy mode**, which observes a live upstream `tools/list` and
emits that artifact with honest completeness, while never executing tools through the proxy (see
[mcp-upstream-proxy-mode.md](mcp-upstream-proxy-mode.md)).

**Experiment-only (characterized, not a shipped feature):** the credential-overbreadth distribution
(the scope lattice is a static model, not a provider-verified taxonomy); MCP tool lifecycle; and an
OTel log-based event projection.

**Parked (needs a separate design before any code):** granular per-tool manifest drift; and the
enforcing `tools/call` proxy (a heavier security boundary — caller authorization, upstream credential
use, a policy decision before forwarding, confused-deputy prevention — specified as a review-spec in
[mcp-proxy-enforcement.md](mcp-proxy-enforcement.md), no code yet).

The load-bearing boundary for the manifest line: **live upstream observation was not a small wiring
step.** `assay-mcp-server` terminates the protocol and serves its own tools, so observing an upstream
manifest on the wire needed a dedicated mode, delivered in v3.23.0 as the opt-in
[manifest-observation proxy](mcp-upstream-proxy-mode.md) (it forwards a tiny allowlist, observes
`tools/list` with honest completeness, and never forwards `tools/call`). The artifact/file-based path
remains for supplied artifacts, and the producer is still not wired to the server's own served tools —
they are not an observed upstream manifest. See [mcp-manifest-drift.md](mcp-manifest-drift.md) for the
topology finding that led here.
