# Self-Correcting Agents

Use Assay's stdio MCP review tools to check proposed arguments and call order.
Your application owns the proposed correction, retry limit and target-tool execution.
Assay currently returns `suggested_fix: null`; it does not generate or apply a fix.
An allow result is a policy decision for the supplied input, not proof of execution,
provider success or safety.

## Start the review server

Save the policy files below in your project, then have your MCP client start:

```bash
assay-mcp-server --policy-root .
```

Use the project's absolute directory instead of `.` when the host starts processes
from another working directory. For a host using `mcpServers` configuration:

```json
{
  "mcpServers": {
    "assay": {
      "command": "assay-mcp-server",
      "args": ["--policy-root", "/absolute/path/to/project"]
    }
  }
}
```

The review server exposes the Assay tools described here. `assay mcp wrap` is a separate
proxy for another MCP server; it is not the server that provides these review tools.
See [MCP runtime commands](../reference/cli/mcp-server.md) for that distinction.

## Available Tools

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

## Policies for Self-Correction

### Argument limits

Save as `discounts.yaml` under the policy root:

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

Save as `sequence.yaml` under the same root:

```yaml
# sequence.yaml
- type: before
  first: verify_identity
  then: delete_customer

```

## Integration Examples

### Python Agent

This synchronous application helper assumes `call_check(name, arguments)` performs an
initialized MCP `tools/call` exchange and returns the **raw JSON-RPC response** as a dict.
It is an application callback, not a particular SDK method. `execute_tool` is your
application's target-tool executor. If your SDK returns a `CallToolResult` object,
adapt that boundary explicitly; do not treat it as the decoded policy decision.

The server places its decision JSON in `result.content[0].text`. Both a policy denial
and a tool failure set `result.isError: true`; inspect the decoded diagnostic fields
to distinguish them. A JSON-RPC `error` is another failure path. Neither is permission
to execute. Do not rely on `structuredContent` for ordinary policy decisions.

```python
import json


def checked_call(call_check, execute_tool, tool_name, arguments):
    """Check these exact arguments; execute only after an explicit allow."""
    reply = call_check("assay_check_args", {
        "tool": tool_name,
        "arguments": arguments,
        "policy": "discounts.yaml",
    })
    if "error" in reply:
        raise ValueError("MCP request failed")
    result = reply["result"]
    decision = json.loads(result["content"][0]["text"])
    if result.get("isError") is not False or decision.get("allowed") is not True:
        raise ValueError(f"Policy did not allow this call: {decision}")
    return execute_tool(tool_name, arguments)

```

A refusal raises before target execution, including a denial with `suggested_fix: null`.
Malformed responses also stop the call. Let the application surface the error or propose
new arguments; call `checked_call` again for every proposal. Never reuse an earlier allow
for changed arguments or dispatch the original call after a failed retry.

Keep retries bounded and log denials separately from accepted proposals and actual outcomes.
Checking a sequence does not replace checking the next call's arguments. These preflight
checks do not intercept a caller that chooses to bypass them.

## See Also

- [Self-Correction Use Case](../use-cases/self-correction.md)
- [MCP API](../reference/mcp-api.md)
- [Policies](../concepts/policies.md)
