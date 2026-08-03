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
| `redaction_failed` | a value that had to be projected could not be safely redacted (reserved) |
| `not_observed` | the tool path was outside the proxy; nothing observed |

The classifier is total: every observed call yields exactly one of these states, never nothing. Each
decision also carries a machine-readable `reason_code` so downstream never parses prose.

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
      "reason_code": "classified_github_deploy_key",
      "action": {
        "class": "privileged_admin_action",
        "verb": "create",
        "resource_type": "github_deploy_key",
        "target": {
          "provider": "github",
          "owner": "org",
          "repo": "prod-repo",
          "key_title_hash": "sha256:...",
          "read_only": false
        }
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
      },
      "correlation": {
        "basis": "propagated_trace_context",
        "traceparent": "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01",
        "source_class": "propagated"
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

The classifier reads arguments only to project the named target fields below. Everything else, every
unknown field and every secret-like value, is dropped, never copied. `owner` and `repo` are plain
labels; principal-like identifiers are hashed (see Redaction).

### `github_deploy_key`

- Tool leaf names: `add_deploy_key`, `create_deploy_key`.
- Required argument fields: `owner`, `repo` (a missing one yields `classified_incomplete`,
  `reason_code: missing_required_target_field`, `detail: missing_github_owner_or_repo`).
- Target projection: `owner`, `repo` (plain), `key_title_hash` (the title is hashed, never stored),
  `read_only` flag if present. `resource_type: github_deploy_key`.
- Dropped, never hashed: `public_key`, `private_key`, `token`, and the like.
- Non-claims: does not store public or private key material; does not prove the key works; does not
  prove GitHub persisted it without audit confirmation.

### `slack_add_member`

- Tool leaf names: `add_member`, `invite`.
- Required fields: a scope (`workspace_id` and/or `channel_id`) plus a principal (`user_id` / `user`).
- Target projection: `workspace_id_hash`, `channel_id_hash` (null for workspace-level membership),
  `principal_hash`. All are hashed under their own domains. `resource_type: workspace_member`.
- Non-claims: does not prove Slack accepted the membership unless verified response/audit evidence;
  does not store tokens or raw principals.

### `workspace_admin`

- Tool leaf names (a deliberately narrow set): `grant_admin`, `change_role`, `invite_external`,
  `modify_org_policy`, `create_workspace_token`.
- Required fields: a workspace (`workspace_id` / `workspace` / `org`) plus a principal.
- Target projection: `workspace_id_hash`, `principal_hash`, `role` (plain label if present).
  `resource_type: workspace_role`.
- Anything outside this verb set is `observed_unknown_tool`; the classifier does not guess.

## Redaction and sanitization

- Raw secrets and tokens never appear in the record. A credential is referenced by a stable alias
  (`credential_alias`), and `secret_material_stored` is always `false`.
- Sensitive identifiers (principals, workspace/channel ids, key titles) are not stored verbatim; they
  are hashed under a **domain-separated** preimage `assay.tool_target.v0:<domain>:<normalized>`, so a
  hash from one field can never collide with another. This is pseudonymization, not anonymization:
  equal inputs yield equal hashes, so the only claim is that the raw value is not stored.
- Secret-like values (`public_key`, `private_key`, `token`, `authorization`, `secret`, `credential`,
  ...) are **dropped, not hashed**: a hash of a public key can still leak correlation, and a token
  hash invites offline brute force.
- Hostile strings (terminal escapes, control characters) are sanitized before the record is written,
  the same discipline the evidence TUI/rendering already applies.

## Correlation basis

MCP 2026-07-28 removed protocol-level sessions (SEP-2567), so "these N records belong to one
interaction" is no longer a transport fact a reader may assume. Each record therefore types the
basis it actually retains, in `correlation`:

| `basis` | meaning |
|---|---|
| `propagated_trace_context` | the request carried a `traceparent` in `_meta` (SEP-414) that passes validation (four lowercase-hex fields `2-32-16-2`, non-zero trace/parent ids, version not `ff`); it is retained verbatim. `source_class: propagated` — a producer-propagated **claim**, never an observed transport fact. A covering, uniform trace-id can claim-support a grouping and a partitioned one can refute it; it cannot be lifted to proof. |
| `malformed_trace_context` | a carrier was sent but does not pass that validation (wrong shape, non-hex, all-zero ids, hostile bytes, a non-string JSON value, or a future-version form this validator is deliberately stricter than W3C about). Its bytes are **not** retained. Distinct from `none` by design: a broken carrier and an absent carrier are different facts. |
| `none` | the record is stateless; no carrier was sent, and the record's own `source_class` is null — it retains no basis to classify. Any grouping of such records rests on producer-minted envelope identity (e.g. a run id) — a producer assertion that lives outside this record's `correlation` field. |

The `source_class` vocabulary of this record family is exactly `"propagated"` or JSON null; it
deliberately borrows nothing from the gateway-evidence or tool-decision-truth source-class
families (a different record family's vocabulary MUST NOT be used here). The consumer rule is a
no-upgrade rule, not an ordering: a grouping claim over several records can never assert a
stronger basis than any member retains, and a group containing a `none` or
`malformed_trace_context` member cannot be grouped on trace context at all — that grouping claim
is incomplete, not weakly supported.

`tracestate` and `baggage` are deliberately not retained: their values are free-form and may carry
data the redaction rules above cannot reason about.

Era scoping: this server still negotiates only legacy handshakes (`2024-11-05` / `2025-11-25`);
for such clients a `_meta.traceparent` is optional practice rather than SEP-414 conformance, so
`basis: "none"` is the expected common case. The extraction does not gate on era: any client that
does send the carrier gets it typed.

## Reason codes

Machine-readable, never parsed from prose: `classified_github_deploy_key`,
`classified_slack_add_member`, `classified_workspace_admin`, `missing_required_target_field`,
`unknown_tool_name`, `redacted_secret_argument`, `unsupported_argument_shape`.

## Reference fixtures

`crates/assay-mcp-server/tests/fixtures/tool_decisions/`:

- `github_deploy_key_allow.json` — classified, allowed, side effect asserted not verified
- `github_deploy_key_deny.json` — classified, denied by policy
- `github_deploy_key_incomplete.json` — `classified_incomplete` (missing `repo`)
- `slack_add_member_allow.json` — classified, allowed
- `workspace_admin_allow.json` — classified, allowed (one concrete tool)
- `unknown_tool_observed.json` — `observed_unknown_tool`, never clean
- `redacted_and_sanitized.json` — secret alias only, control chars sanitized; carries the
  `malformed_trace_context` correlation state (carrier sent but invalid, bytes dropped)

Every fixture carries a `correlation` object; between them the vectors cover all three basis
states (`propagated_trace_context`, `malformed_trace_context`, `none`).
