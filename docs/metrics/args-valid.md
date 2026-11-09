# args_valid

Check every tool call in a recorded trace against a policy: which tools may be called, and a JSON
Schema for each tool's arguments.

---

## Synopsis

```yaml
tests:
  - id: validate_args
    input:
      prompt: "Apply a discount to order ord_123"
    expected:
      type: args_valid
      policy: policies/customer.yaml
```

`policy:` is read relative to the directory `assay run` runs in. Put the policy inline under
`schema:` instead of `policy:` for small cases.

---

## Description

For each tool call in the trace, `args_valid` checks:

- that the tool is not denied, and is allowed when the policy has an allow list;
- that the call's arguments validate against the tool's JSON Schema under `schemas.<tool>`;
- what to do with an allowed tool that has no schema (`enforcement.unconstrained_tools`).

A call that breaks the policy fails the test. The schemas are standard JSON Schema; see
[MCP Policy Files](../reference/config/policies.md#json-schema-in-schemas) for the keywords and
shared `$defs`.

---

## Example policy

```yaml
# policies/customer.yaml
version: "2.0"

deny: [delete_customer]

schemas:
  apply_discount:
    type: object
    additionalProperties: false
    properties:
      percent:
        type: number
        minimum: 0
        maximum: 30
      order_id:
        type: string
        pattern: "^ord_[0-9]+$"
    required: [percent, order_id]

enforcement:
  unconstrained_tools: warn
```

A trace call `apply_discount` with `{"percent": 50, "order_id": "ord_123"}` fails the test,
because `percent` exceeds `maximum: 30`. `assay run` exits 1 and names the failed test;
`assay validate --trace-file` reports it as `E_ARGS_VIOLATION`.

### Inline schema

```yaml
tests:
  - id: discount_limit
    input:
      prompt: "Apply a discount"
    expected:
      type: args_valid
      schema:
        apply_discount:
          type: object
          properties:
            percent: { type: number, maximum: 30 }
          required: [percent]
```

An inline `schema:` is either a tool-name-to-schema map, as here, or a whole policy with
`version: "2.0"`.

---

## Configuration errors

These are refused before the test runs (exit 2, `E_CFG_PARSE`), by `assay run`, `assay ci`,
`assay validate` and `assay doctor`:

- a policy that constrains nothing at all;
- a policy field this check does not read: a root field other than `version`, `name`, `allow`,
  `deny`, `tools`, `schemas` and `enforcement`, a `tools.*` list other than `allow` and `deny`, or
  an `enforcement` key other than `unconstrained_tools`. That covers a misspelled field, and the
  fields `assay mcp wrap` reads and an `args_valid` test does not, such as `constraints`, `limits`
  and `tool_pins`;
- a tool schema with a root key that is not a JSON Schema keyword. A map of argument names to
  constraints is the common case: write it under `properties`, as in the example above. Prefix a key
  that is only an annotation with `x-`;
- a tool schema that asserts nothing without saying so. Write `<tool>: true` for a tool whose
  arguments may be anything.

Each tool schema is checked on its own, so a deny list or another tool's schema does not make a
schema that constrains nothing acceptable.

`format` is an annotation under the default JSON Schema dialect: `format: email` does not reject a
value. It is checked when the tool schema declares `$schema: "http://json-schema.org/draft-07/schema#"`;
otherwise use `pattern` to constrain a string's shape.

---

## See Also

- [MCP Policy Files](../reference/config/policies.md)
- [Policies](../concepts/policies.md)
- [sequence_valid](sequence-valid.md)
- [tool_blocklist](tool-blocklist.md)
