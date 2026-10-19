# Agent Self-Correction

An agent can ask Assay to review a proposed call before the application executes it.
For example, a discount proposal of 50 can be denied by a policy whose maximum is 30.
The application may then propose 30 and ask again. Only an explicit allow for that
new proposal permits the application's execution step.

Assay does not choose the replacement value: `suggested_fix` is currently `null`.
A denial is useful feedback, not an instruction to execute a modified call. An allow
does not guarantee tool success or establish that an external effect occurred.

## Setup

Save the policies below and start the review server through your MCP client:

```bash
assay-mcp-server --policy-root .
```

The policy root must be the directory holding those files. This is the standalone
review server, not `assay mcp wrap`. See the [integration example](../mcp/self-correction.md#python-agent)
for raw JSON-RPC decoding and a helper that refuses execution on policy denial or MCP errors.

## Available Checks

### assay_check_args

Pass these arguments to the `assay_check_args` tool through your MCP client:

```json
{
  "tool": "apply_discount",
  "arguments": {"percent": 50},
  "policy": "discounts.yaml"
}
```

The policy denies 50. Selected fields from the decoded tool result are:

```json
{"allowed": false, "suggested_fix": null}
```

The response also carries diagnostic fields; it does not provide corrected arguments.
An application may propose `{"percent": 30}`, but must submit those new arguments
for another check. With this policy that proposal returns `allowed: true`.

### assay_check_sequence

Pass these arguments to `assay_check_sequence`:

```json
{
  "history": [],
  "next_tool": "delete_customer",
  "policy": "sequence.yaml"
}
```

With the sequence policy below, deletion without the prerequisite is denied.
Selected decoded fields are again:

```json
{"allowed": false, "suggested_fix": null}
```

The `violations` array describes the violated rule. After the application has actually
observed `verify_identity`, `history: ["verify_identity"]` permits this next step.
History is caller-supplied context, not independently authenticated execution evidence.
Do not invent a successful prerequisite to obtain an allow result.

### assay_policy_decide

`assay_policy_decide` performs an exact-name check against its compatibility-only root
`blocklist`. It does not parse full `McpPolicy` controls such as `tools.allow` or
`tools.deny`; use `assay_check_args` for full, argument-aware policy evaluation.
Passing canonical name-policy fields to `assay_policy_decide` is an error, not a clean allow.

## Real Example: E-commerce Agent

### Policy

Save as `discounts.yaml`:

```yaml
# discounts.yaml
version: "2.0"
name: discounts
tools:
  allow: [apply_discount]
schemas:
  apply_discount:
    type: object
    additionalProperties: false
    properties:
      percent:
        type: number
        minimum: 0
        maximum: 30
    required: [percent]

```

### Sequence Prerequisites

Save as `sequence.yaml`:

```yaml
# sequence.yaml
- type: before
  first: verify_identity
  then: delete_customer

```

## Application-owned correction

1. Submit the original arguments to `assay_check_args`.
2. On an MCP error, stop and resolve the failure. Do not interpret it as an allow.
3. On a policy denial, retain the feedback. The application can stop, ask the user,
   or propose different arguments within its own bounded retry policy.
4. Recheck each changed proposal. Execute only when that proposal receives `allowed: true`.
5. Record the actual tool outcome separately; a preflight decision is not that outcome.

For the policy above, 50 is denied, 40 is still denied, and 30 is allowed. No target call
should execute during the first two checks. Do not merge `suggested_fix` into an argument
mapping: the shipped value is `null`, and no automatic correction is supplied.

Sequence checks use observed history so far. Do not preflight an entire plan using imagined
successful prerequisites and then treat later steps as already authorized.

## Monitoring

Track denied proposals, retries, accepted proposals and actual execution failures separately.
Their rates depend on your workload and policy; these examples establish no error-rate
reduction or guarantee of successful execution.

## See Also

- [MCP Self-Correction Integration](../mcp/self-correction.md)
- [MCP Runtime Commands](../reference/cli/mcp-server.md)
- [Policies](../concepts/policies.md)
