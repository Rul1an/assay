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
  -> did the tool manifest drift?             (manifest-drift, experiment)
```

## Record types

| Record | What it carries | Reference |
|--------|-----------------|-----------|
| `assay.tool_decision_surface.v0` | per-call: server, classified action + projected target, decision, response, redaction | [tool-decision-surface.md](tool-decision-surface.md) |
| `assay.declared_tool_surface.v0` | declared/allowed privileged actions, for observed-vs-declared review | [declared-tool-surface.md](declared-tool-surface.md) |
| `action.required_scope` (+ declared credentials) | the scope an action requires vs the alias's declared scope | [credential-scope.md](credential-scope.md) |
| `assay.provider_audit_record.v0` | an imported provider audit entry, bound to an observed call by digest recompute | [side-effect-receipt.md](side-effect-receipt.md) |

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

## What is shipped vs experiment-only

Shipped (releasable): the tool-decision surface, the rule-based classifiers, the declared-vs-observed
gate, credential-scope evidence, the side-effect receipt spec, and the Plimsoll consumer that ranks
the side-effect ladder.

Characterized but kept experiment-only for now (not in a shipped feature): the credential-overbreadth
distribution (the scope lattice is a static model, not a provider-verified taxonomy), and MCP
tool-manifest drift detection. Manifest drift is the next candidate to productize as a feature; a
legitimate manifest change surfaces as drift and resolves to `pending_tool_manifest_review`, never a
maliciousness verdict.
