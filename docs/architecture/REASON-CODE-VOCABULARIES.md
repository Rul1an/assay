# Reason-code vocabularies and the surfaces they reach

Status: reference inventory. Machine-checked by
`crates/assay-cli/tests/reason_code_vocabularies.rs`.

This file records every reason-code-shaped vocabulary in the workspace and, for
each, **the artifact field it writes to**. It does not decide whether any of
them should merge — that is #2028's job. It exists so the count stops growing
unrecorded while that is decided, and so that adding a code to one of the
vocabularies that shares a surface forces a decision about the others.

## Why surface, not similarity

Two vocabularies conflict when they can appear in **the same field of the same
artifact under the same tool identity**. Similar names are not a conflict.
`assay_cli::enforcement_health_v1::ReasonCode` is a snake_case serde field
inside an evidence block; it is never a SARIF `ruleId`, never a
`Diagnostic.code`, and never an exit reason. It is a homonym, and listing it as
a collision would overstate the problem.

The discriminator is the surface. So the inventory is organised by surface.

## Surface 1 — SARIF `ruleId` under `tool.driver.name = "assay"`

**Four sources write to this one field.** Only the first is a registry.

| Source | Members | Reaches it via |
|---|---|---|
| `assay_core::errors::diagnostic::codes` | 12, listed below | `Diagnostic.code` → `build_sarif_diagnostics` (`report/sarif.rs:217`) |
| `assay_core::policy_engine` verdict codes | 5 blocking, listed below | forwarded verbatim into `Diagnostic::new` (`validate/mod.rs:219`) |
| bare literal `"E_UNKNOWN"` | 1 | `validate/mod.rs:136`, for an unexpected trace error |
| bare literal `"assay"` | 1 | `write_sarif` (`report/sarif.rs:104`), the generic id for every test result |

`assay validate --format sarif` emits the first three; `assay ci --sarif` emits
the fourth. Different files, one `tool.driver.name`, so one alert namespace in
GitHub Code Scanning.

Neither producer populates `tool.driver.rules[]`, so every id in this namespace
is a `ruleId` with no rule descriptor behind it — no name, no description, no
help URI. The lint producer on surface 2 does populate it.

<!-- machine-checked: diagnostic-codes -->
```text
E_BASE_MISMATCH
E_CFG_PARSE
E_CFG_SCHEMA
E_EMB_DIMS
E_PATH_NOT_FOUND
E_POLICY_VIOLATION
E_REPLAY_STRICT_MISSING
E_TRACE_INVALID
E_TRACE_MISS
W_BASE_FINGERPRINT
W_CACHE_CONFUSION
W_CFG_VACUOUS_EXPECTED
```

<!-- machine-checked: policy-engine-codes -->
```text
E_ARG_SCHEMA
E_POLICY_MISSING_TOOL
E_POLICY_REGEX_INVALID
E_SCHEMA_COMPILE
E_SEQUENCE_VIOLATION
OK
```

`E_TOOL_NOT_ALLOWED` is deliberately absent. The field's own comment named it as
an example value, and this list was first written with it included on that
basis; the test rejected it, because `policy_engine.rs` never constructs it. The
code is live, but in surface 6 — a different vocabulary reaching a different
artifact. The comment has been corrected to stop pointing at it.

`OK` is the `reason_code` of a non-blocked verdict. It never reaches a
`Diagnostic`, because `validate/mod.rs:216` forwards only `VerdictStatus::Blocked`.
It is listed because it is a member of the vocabulary, and the parser that
produces this list must not have to know which members are reachable — a parser
that filtered by reachability would silently stop covering a code the day the
forwarding condition changed.

Of these five blocking codes, `E_ARG_SCHEMA` and `E_SEQUENCE_VIOLATION` appear
in `SPEC-PR-Gate-Outputs-v1.md` §5.3 and in `assay_cli::exit_codes::ReasonCode`.
The other three — `E_POLICY_MISSING_TOOL`, `E_POLICY_REGEX_INVALID`,
`E_SCHEMA_COMPILE` — appear in **no** registry, and reach a published SARIF
artifact. See #2027.

## Surface 2 — SARIF `ruleId` under `tool.driver.name = "assay-evidence-lint"`

| Source | Members | Reaches it via |
|---|---|---|
| `assay_evidence::lint::rules::RULES` | 6, listed below | `to_sarif_with_options` (`lint/sarif.rs:392` sets the driver name) |

<!-- machine-checked: lint-rule-ids -->
```text
ASSAY-I001
ASSAY-W001
ASSAY-W002
ASSAY-W003
ASSAY-W004
ASSAY-W005
```

**This is a separate namespace, not a collision with surface 1.** The driver
name differs, so GitHub Code Scanning keys these alerts separately. The
inventory asserts the two id sets stay disjoint anyway, so that if the driver
names are ever unified the invariant is already held rather than discovered.

`ASSAY-LINT-TRUNCATED` (`lint/sarif.rs:372`) is not in this set. It is a
`toolExecutionNotifications[].descriptor.id`, a different field, and it is
deliberately not a rule.

## Surface 3 — `reason_code` in `run.json` and `summary.json`

| Source | Reaches it via |
|---|---|
| `assay_cli::exit_codes::ReasonCode` | `as_str()`, specified by `SPEC-PR-Gate-Outputs-v1.md` §5.1–5.2 |

Versioned by `REASON_CODE_VERSION` and contract-tested. Governed by the spec's
§175 rule: existing codes must not be removed or repurposed without a
`schema_version` bump and migration notes.

Not machine-checked here. `ReasonCode` is an enum with an exhaustive `as_str`,
so the compiler already fails on an unhandled variant — the drift this file
guards against is the kind a compiler cannot see.

## Surface 4 — evidence-block serde fields

These are snake_case values inside evidence JSON. None reaches a SARIF
`ruleId`, an exit code, or a `Diagnostic.code`. They share the *name* "reason
code" and nothing else.

| Source | Field it writes |
|---|---|
| `assay_evidence::types::SandboxDegradationReasonCode` | sandbox degradation record |
| `assay_cli::enforcement_health_v1::ReasonCode` | `EnforcementHealthV1` block |
| `assay_core::mcp::obligations::REASON_CODE_*` | obligation outcome record |
| `assay_core::mcp::decision_next::normalization::OUTCOME_REASON_CODE_*` | normalized obligation outcome |

## Surface 5 — policy decision identifiers

| Source | Field it writes |
|---|---|
| `assay_runner_core` `P_TOOL_ALLOWED` (`assay-runner-core/src/policy.rs:266`) | runner policy decision record |

A single identifier, SCREAMING_SNAKE like surface 1 but reaching a different
artifact entirely.

## Surface 6 — MCP policy decision codes

| Source | Field it writes |
|---|---|
| `assay_core::mcp::policy::engine_next::precedence` `E_*` codes | MCP policy decision `code` |
| `assay_core::mcp::decision_next::event_types::reason_codes` `P_*` | MCP decision event `reason_code` |

The `E_*` half is a third SCREAMING_SNAKE `E_`-prefixed vocabulary, distinct
from both surface 1 sources: `E_TOOL_NOT_ALLOWED`, `E_TOOL_DENIED`,
`E_TOOL_DRIFT`, `E_RATE_LIMIT`, and `E_ARG_SCHEMA` — the last shared by name
with surface 1 and mapped here to `P_ARG_SCHEMA`.

`map_policy_code` (`mcp/proxy/decisions.rs:65-70`) translates them into the
`P_*` snake_case space with a `_ => P_POLICY_DENY` catch-all, so an unrecognized
policy code becomes a generic deny rather than an error. That is the same shape
as the severity default #2025 removed, on a different surface; recorded here
rather than changed, because the deny direction is fail-closed and the decision
belongs in #2028.

Neither half reaches a SARIF `ruleId`. `assay_core::agentic::builder:153`
*matches* on `E_TOOL_NOT_ALLOWED` to pick a remediation string; it does not
construct a `Diagnostic`.

## What the test enforces

`crates/assay-cli/tests/reason_code_vocabularies.rs`:

1. The `diagnostic-codes` block equals the `pub const` set parsed out of
   `assay_core::errors::diagnostic::codes`.
2. The `policy-engine-codes` block equals the `reason_code:` set parsed out of
   `assay_core::policy_engine`, and the parse **fails** on a `reason_code:`
   whose value is not a string literal rather than skipping it.
3. The `lint-rule-ids` block equals `RULES[].id`, read at runtime.
4. Surfaces 1 and 2 are disjoint, and neither contains the generic `"assay"`
   id that `write_sarif` emits into the same driver namespace.

Adding a code to any of the three machine-checked vocabularies fails the test
until this file is updated. Updating this file is the recorded decision.
