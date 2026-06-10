# Declared tool surface (`assay.declared_tool_surface.v0`)

Status: spec + reference fixtures (P58a). The declared-vs-observed diff and gate (P58b) consume this
alongside the observed `assay.tool_decision_surface.v0`.

P57 records what privileged tool actions actually happened. P58 compares that to what was declared.
This is the declared side: the set of privileged tool actions an operator says are expected and
allowed for a run.

## What "declared" is and is not

A declared tool surface is an **allowance / expected surface**. It is not enforcement, and it is not
proof. It states "these privileged actions, against these targets, are expected here." Whether a run
actually stayed within it is decided by the diff (P58b), and whether enforcement was active is a
separate carrier (`enforcement_health`). Declaring an action does not make it safe; it only makes its
appearance unsurprising.

## Shape

```json
{
  "schema": "assay.declared_tool_surface.v0",
  "declared_tool_actions": [
    {
      "id": "github.deploy_key.prod",
      "provider": "github",
      "action_class": "github_deploy_key",
      "allowed_targets": [
        { "owner": "org", "repo": "prod-repo" }
      ],
      "allowed_effects": ["allow", "deny"],
      "required_decision": "explicit_policy"
    }
  ]
}
```

- `action_class` matches the observed record's `tool.category` (`github_deploy_key`,
  `slack_add_member`, `workspace_admin`).
- `allowed_targets` is a list of target constraints. A field set to `"*"` or omitted means "any
  value for that field". An empty list means "the action is declared, with no target constraint".
- `allowed_effects` is a subset of `["allow", "deny"]`.
- `required_decision` records the expected decision discipline (e.g. `explicit_policy`); it is
  metadata for review, not a match key.

## Matching an observed decision against a declared action

An observed tool decision **matches** a declared action when all hold:

1. `provider` is equal;
2. the observed `tool.category` equals the declared `action_class`;
3. the observed `action.target` satisfies at least one `allowed_targets` entry;
4. the observed `decision.effect` is in `allowed_effects`.

Target satisfaction is field-wise. A declared field of `"*"` or absent matches anything. For plain
fields (github `owner`, `repo`) the declared value must equal the observed value. For fields the
observed record carries **hashed** (slack and workspace ids, principals), the declared value is the
plain value, and the matcher hashes it with the same domain-separated function
(`assay.tool_target.v0:<domain>:<normalized>`) before comparing to the observed hash. A declared
hash (`sha256:...`) may also be given directly. This keeps the declared surface writable in plain
terms while the observed record never stores the raw value.

## Diff outcomes (implemented in P58b)

The diff is honest about coverage; it never reads absence as safety.

| Situation | Outcome |
|-----------|---------|
| observed, declared, allowed target | no finding |
| observed undeclared privileged action | finding: `new_privileged_tool_action` |
| observed action with incomplete target | `inconclusive_tool_target` |
| observed denied privileged action | `attempted_privileged_tool_action_denied` (finding or warning) |
| declared but unobserved | no regression finding |
| observation gap in the MCP proxy | `inconclusive_observation_gap` |
| unknown tool with high-risk shape | `unclassified_tool_action` |

The load-bearing rule: **no observed tool calls is not the same as no tool capability**. Only
"no observed tool calls plus complete tool observation" is "no observed tool use in this run". The
diff and gate therefore depend on a coverage signal (`tool_observation_health`: proxy seen, tool
calls observed, policy layer active, classifier version, unknown-tool count, redaction active),
reusing `observation_health.policy_layer` where it can express the gap rather than inventing a
second health carrier.

## Non-claims

- a declared action is an allowance, not enforcement and not proof of safety;
- a declared-but-unobserved action is not a regression and not proof the action cannot happen;
- the diff does not prove SaaS-side persistence and does not infer tool calls outside observed proxy
  traffic.

## Reference fixtures

`crates/assay-mcp-server/tests/fixtures/declared_tool_surface/`:

- `declared_prod_allowlist.json` — declares the github deploy-key (prod repo), slack add_member, and
  workspace_admin actions, so the matching observed allows produce no finding.
- `declared_empty.json` — declares nothing, so every observed privileged action is undeclared.
