# Assertion Types

Behavioral checks are defined using **inline assertions** on the test case, rather than separate
policy files.

Every type and field below is taken from the executable enum
(`crates/assay-core/src/agent_assertions/model.rs`). Every example on this page is deserialized
into that enum by a test, so a type or field that stops existing fails CI rather than drifting
into prose.

An assertion carrying a key that is not listed here is **rejected at parse time**. An unknown key
is either a typo or a feature that does not exist, and silently dropping it can turn a real check
into one that cannot fail.

---

## Tool Assertions

### `trace_must_call_tool`

Passes if the trace contains at least `min_calls` calls to the named tool. `min_calls` defaults
to 1.

```yaml
type: trace_must_call_tool
tool: get_weather
min_calls: 1
```

The count is over recorded tool calls. It does **not** distinguish a call that succeeded from one
that errored: `ToolCallRow` (`crates/assay-core/src/storage/rows.rs`) carries no status or error
column, so an errored call satisfies this assertion. There is no `max_calls` field — to express
"must not be called", use `trace_must_not_call_tool` below.

### `trace_must_not_call_tool`

Passes if the trace contains **zero** calls to the named tool.

```yaml
type: trace_must_not_call_tool
tool: delete_database
```

This is the only way to express a forbidden tool. Do not attempt it with `trace_must_call_tool`
and a count of zero.

### `tool_blocklist`

Checks a list of tool calls against a blocked list. Useful as a unit test of the policy itself,
without a recorded episode.

```yaml
type: tool_blocklist
test_tool_calls: [read_file, delete_database]
policy:
  blocked: [delete_database]
expect: fail
```

`policy.blocked` must be present and non-empty; an absent or empty blocklist admits every call
and is rejected as ineffective.

---

## Argument Assertions

### `args_valid`

Checks tool arguments against a policy. The policy may carry a JSON Schema, which covers both
value matching and structural validation.

```yaml
type: args_valid
tool: apply_discount
policy:
  schema:
    properties:
      percent: { type: number, maximum: 30 }
      reason: { type: string }
    required: [percent]
test_args:
  percent: 10
  reason: "Loyalty program"
expect: pass
```

`expect` accepts `pass` or `fail` and nothing else; any other spelling is rejected rather than
being read as its opposite.

---

## Sequence Assertions

### `trace_tool_sequence`

Requires the named tools to appear in the given relative order. `allow_other_tools` is
**required** — there is no default, and it decides whether other calls may appear in between.

```yaml
type: trace_tool_sequence
sequence: [login, view_balance, logout]
allow_other_tools: true
```

With `allow_other_tools: false` the sequence must match exactly. An empty `sequence` with
`allow_other_tools: false` is the effective "no tool calls at all" constraint; an empty sequence
with `allow_other_tools: true` constrains nothing and is rejected.

### `sequence_valid`

Checks a supplied trace against a sequence policy. Like `tool_blocklist`, this is a unit-test
form that needs no recorded episode.

```yaml
type: sequence_valid
test_trace_raw:
  - tool: submit_payment
    args: { iban: "DE00" }
policy:
  regex: '^submit_payment$'
expect: pass
```

Supply the trace through `test_trace_raw`. The policy must carry a usable `regex`; a policy
without one is universally permissive and is rejected.

---

## Step Assertions

### `trace_max_steps`

Passes if the episode used no more than `max` steps.

```yaml
type: trace_max_steps
max: 10
```

---

## Not implemented

These types have appeared in earlier drafts of this page. They are **not** in the enum and are
rejected. The nearest shipped equivalent is given so a config can be written today:

| Not implemented | Use instead |
|---|---|
| `trace_no_tool_call` | `trace_must_not_call_tool` |
| `trace_tool_args_match` | `args_valid` with a `policy.schema` |
| `trace_tool_args_schema` | `args_valid` with a `policy.schema` |
| `trace_tool_call_count` | `trace_must_call_tool` with `min_calls` — an upper bound is not expressible |
| `trace_no_tool_errors` | nothing; recorded tool calls carry no outcome, so this cannot be evaluated |

---

## Relationship to the metric names

`args_valid`, `sequence_valid`, and `tool_blocklist` are the names used both as assertion types
here and as metric names on the `expected:` surface. They are the same checks reached two ways;
the names do not diverge.
