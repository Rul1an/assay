# Tool-decision surface (`assay.tool_decision_surface.v0`)

Status: spec + reference fixtures (P57a). No producer wired yet; the `assay-mcp-server` observation
(P57b) and the classifiers (P57c) build on this shape, and the declared-vs-observed gate (P58)
consumes it.

## Why this exists

Kernel and network enforcement see that an agent connected to `api.github.com:443`. They do not see
that the agent, through an MCP tool call, added a deploy key to a production repository or added an
external user to a Slack workspace. Those privileged in-application actions are the kernel-blind gap.
The layer that can observe them is the MCP proxy (`assay-mcp-server`), which sees each `tools/call`,
the policy decision it took, and the response.

`capability_surface.v0` already records observed MCP tools, but only as a flat, deduplicated set of
tool-name strings plus decision strings (`mcp_tools`, `policy_decisions`). It cannot carry a
structured per-call record: server identity, the classified action and its target, the
asserted-versus-verified status of the side effect, or redaction state. So this is a new, explicit
carrier rather than an overload of the capability surface.

## Claim and non-claims

**Claim:** Assay records observed MCP tool decisions as evidence, including privileged-action
classification where the proxy can determine it.

**Non-claims (global):**

- does not prove the external SaaS side effect happened or persisted without independently verified
  audit evidence;
- does not infer tool actions outside observed MCP proxy traffic;
- does not expose raw secrets or tokens;
- does not replace the provider's own audit log.

## The load-bearing rule: asserted vs verified

This is the rule the whole surface is built to keep honest.

| Layer | Status |
|-------|--------|
| observed `tools/call` request | observed |
| proxy policy decision | observed / enforced by the proxy |
| SaaS side effect | **asserted** unless independently verified |
| SaaS audit log | external **verified** evidence, only if imported and checked |

A tool returning `"deploy key added"` is the provider's assertion, not proof. The record may carry
it, but must label it: `response.side_effect_asserted` can be true while
`response.side_effect_verified` stays false. `side_effect_verified` only becomes true when separate,
checked audit evidence confirms it. The surface never silently promotes asserted to verified.

## Classification states

The classifier is honest about what it could and could not determine:

| State | Meaning |
|-------|---------|
| `classified` | a known privileged tool was observed and its target projected |
| `classified_incomplete` | known tool, but required argument fields were missing |
| `observed_unknown_tool` | a tool call was observed but matched no classifier |
| `not_observed` | the tool path was outside the proxy; nothing observed |

An unknown tool is never silently treated as clean, and missing arguments are never treated as safe.
"No observed tool calls" does not mean "no tool capability"; only "no observed tool calls plus
complete tool observation" means "no observed tool use in this run" (see P58 coverage).

## Record shape

```json
{
  "schema": "assay.tool_decision_surface.v0",
  "observed_tool_decisions": [
    {
      "server": {
        "id": "github",
        "transport": "mcp",
        "declared_manifest_digest": "sha256:..."
      },
      "tool": { "name": "github.add_deploy_key", "category": "github_deploy_key" },
      "classification": "classified",
      "action": {
        "class": "privileged_admin_action",
        "verb": "create",
        "resource_type": "github_deploy_key",
        "target": { "provider": "github", "owner": "org", "repo": "prod-repo" }
      },
      "decision": {
        "effect": "allow",
        "source": "assay-mcp-server",
        "rule_id": "tool.github.deploy_key.allow.prod",
        "enforced": true
      },
      "response": {
        "status": "success",
        "side_effect_asserted": true,
        "side_effect_verified": false
      },
      "redaction": {
        "arguments_redacted": true,
        "credential_alias": "github-prod-admin",
        "secret_material_stored": false
      }
    }
  ],
  "non_claims": [
    "does not prove SaaS-side persistence without external audit evidence",
    "does not infer tool actions outside observed MCP proxy traffic",
    "does not expose raw secrets or tokens"
  ]
}
```

## Classifiers (P57c)

Classifiers are rule-based and explicit. No model or judge decides a classification. Start narrow,
with three concrete cases; broaden only with a fixture per added case.

### `github_deploy_key`

- Tool names / aliases: `github.add_deploy_key`, `create_deploy_key`, equivalents.
- Required argument fields: `owner`, `repo` (a missing one yields `classified_incomplete`).
- Target projection: `owner`, `repo`, `key_title_hash` (or redacted title), `read_only` flag if
  present.
- Non-claims: does not store public or private key material; does not prove the key works; does not
  prove GitHub persisted it without audit confirmation.

### `slack_add_member`

- Tool names / aliases: `slack.add_member`, `conversations.invite`, equivalents.
- Required fields: workspace or channel identifier, principal identifier.
- Target projection: `workspace_id` or alias, `channel_id` or user-group, `user_id` hash or redacted
  principal, role/class if present.
- Non-claims: does not prove Slack accepted the membership unless verified response/audit evidence;
  does not store tokens.

### `workspace_admin`

A category for `grant_admin`, `change_role`, `invite_external`, `create_workspace_token`,
`modify_org_policy`. Kept deliberately narrow in P57: one concrete tool fixture, not the whole class.

## Redaction and sanitization

- Raw secrets and tokens never appear in the record. A credential is referenced by a stable alias
  (`credential_alias`), and `secret_material_stored` is always `false`.
- Argument values that carry sensitive identifiers are redacted or hashed, not stored verbatim
  (`arguments_redacted: true`).
- Hostile strings in arguments (terminal escapes, control characters) are sanitized before the record
  is written, the same discipline the evidence TUI/rendering already applies.

## Reference fixtures

`crates/assay-mcp-server/tests/fixtures/tool_decisions/`:

- `github_deploy_key_allow.json` — classified, allowed, side effect asserted not verified
- `github_deploy_key_deny.json` — classified, denied by policy
- `github_deploy_key_incomplete.json` — `classified_incomplete` (missing `repo`)
- `slack_add_member_allow.json` — classified, allowed
- `workspace_admin_allow.json` — classified, allowed (one concrete tool)
- `unknown_tool_observed.json` — `observed_unknown_tool`, never clean
- `redacted_and_sanitized.json` — secret alias only, control chars sanitized
