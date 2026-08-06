# ADR-046: The reason-code registries stay separate, because they were never two answers to one question

- Status: Accepted
- Date: 2026-08-06
- Supersedes: none
- Amends: none — no ADR governs error codes today. ADR-019 references reason codes and defers
  entirely to `SPEC-PR-Gate-Outputs-v1.md`; nothing has ever governed the `codes::` registry.

## Context

#2010 records two registries that overlap, disagree, and are both public-facing:

- `assay_core::errors::diagnostic::codes` — string constants reaching `Diagnostic.code`
- `assay_cli::exit_codes::ReasonCode` — an enum whose `as_str()` is the `reason_code` field in
  `run.json` and `summary.json`, specified by `SPEC-PR-Gate-Outputs-v1.md`

It offers two candidates — one registry with `ReasonCode` as the source of truth, or two registries
with an explicit derivation — and settles neither, because three things had to be measured first.
Those measurements are now done, and two of them changed the question.

### The exit coupling is gone

`decide_exit` used to infer a code's exit class by matching its spelling as a prefix. It now reads
`ERROR_EXIT_CLASSES`, a table the registry owns, and an unregistered code is `ExitClass::Unregistered`
rather than a silent guess. Renaming a code no longer changes an exit code, so a renaming option can
be evaluated on its merits — which is what #2028 was waiting for.

### The vocabularies are recorded, and the collision is not one

`docs/architecture/REASON-CODE-VOCABULARIES.md` (#2026) inventories every reason-code-shaped
vocabulary by **the artifact field it writes to**, and a test holds the three machine-readable ones
to what the code declares.

#2026 was filed on the belief that `Diagnostic.code` and the evidence lint rule ids collide, both
being SARIF `ruleId`. Measured, they do not. The two producers write different `tool.driver.name` —
`"assay"` against `"assay-evidence-lint"` — so GitHub Code Scanning keys their alerts in separate
namespaces. **The premise of #2010's merge question is false for that pair.**

What does share the `"assay"` namespace is three things, only one of which is a registry:

| Source | Members |
|---|---|
| `codes::` | 12 |
| `policy_engine` verdict codes, forwarded verbatim at `validate/mod.rs:219` | 5 blocking |
| the bare literal `"E_UNKNOWN"` at `validate/mod.rs:136` | 1 |

plus `"assay"` itself, the generic `ruleId` `write_sarif` stamps on every test result under the same
driver name from a different command.

### The spec drift is fixed, and the dead codes are recorded

`SPEC-PR-Gate-Outputs-v1.md` §5.4 and §5.5 (#2027) now register what the implementation emits and
record what is declared but dead, under the §175 rule that a registered code is not deleted without
a `schema_version` bump. A conformance test fails when a variant is added without registering it.

## Decision

### 1. The two registries stay separate. They are not merged.

They answer different questions on different surfaces:

| | `codes::` | `ReasonCode` |
|---|---|---|
| surface | SARIF `ruleId` + terminal diagnostics, driver `"assay"` | `reason_code` in `run.json` / `summary.json` |
| cardinality | many per run | one per run |
| severity | has warnings | has none |
| governed by | this ADR | `SPEC-PR-Gate-Outputs-v1.md`, versioned by `REASON_CODE_VERSION` |

**The live overlap is one code.** #2010 names `E_CFG_PARSE` and `E_POLICY_VIOLATION` as present in
both; #2027 established that `codes::E_POLICY_VIOLATION` is constructed nowhere. So a merge would
unify two vocabularies that share exactly `E_CFG_PARSE`, and would have to answer what becomes of
`E_CFG_SCHEMA`, `E_EMB_DIMS`, `E_BASE_MISMATCH`, `E_REPLAY_STRICT_MISSING` and three `W_*` warnings
that `ReasonCode` has no severity for. That is a large migration of a published interface to
deduplicate one string.

Rejecting the merge is not a preference for the status quo. It is what the measurement supports, and
the case for revisiting it is a second shared member, not a second opinion.

### 2. Warnings stay in `codes::`. `ReasonCode` gains no warning severity.

`ReasonCode` is the reason a run ended, and a run does not end because of a warning. Only
`W_CFG_VACUOUS_EXPECTED` is ever emitted (`validate/mod.rs:345`); the other two are reserved in
§5.4. Adding a warning severity to `ReasonCode` would give `summary.json` a `reason_code` at
exit 0, which §5's normative rule is written to prevent.

### 3. `ReasonCode` does not move to `assay-core`.

#2028 names a third option #2010 does not list, and it is already the shape in use:
`assay_core::errors::RunErrorKind` is a typed core vocabulary with a total mapping to `ReasonCode`
at `run_output.rs:6-19`. The core describes what went wrong; the CLI decides what to call it in its
own published artifact. Moving `ReasonCode` down would put a CLI output contract in a library that
does not own that output, and buy nothing the mapping does not already give.

### 4. The near-duplicates: two codes, not three.

#2010 states: *"A missing trace file is `E_TRACE_MISS` or `E_PATH_NOT_FOUND` from `validate`/`doctor`,
and `E_TRACE_NOT_FOUND` from `run`. Three names, one condition."*

Opening the construction site refutes it. `providers/trace.rs:37` builds `E_TRACE_MISS` with the
message *"Trace miss: prompt not found in loaded traces"* — the trace file loaded successfully and a
prompt is not in it. That is a coverage miss, not a missing file, and `validate/mod.rs:108` labels
its block "Trace Coverage" for that reason.

So the near-duplicate pair is:

| Code | Condition | Emitted by |
|---|---|---|
| `E_PATH_NOT_FOUND` | a configured path does not exist | `validate`, `doctor` |
| `E_TRACE_NOT_FOUND` | the trace file does not exist | `run` |

and `E_TRACE_MISS` is a third, genuinely different condition that was miscounted into the pair.

**Decision: neither name is deprecated, and no code is renamed.** They mark the same condition, but
they land in different artifacts read by different consumers — `E_PATH_NOT_FOUND` in a SARIF
`ruleId`, `E_TRACE_NOT_FOUND` in `summary.json` — and unifying the spelling across two artifacts is
the merge rejected in decision 1, arrived at from the other end.

What is fixed instead is the thing that made the duplication harmful: the two used to exit 1 and 2
for the same condition, because the exit class came from the spelling. `ERROR_EXIT_CLASSES` removed
that. A consumer that branches on either code now gets the same exit code, which is what "one
condition, one meaning" has to mean across two artifacts that cannot share a string.

`docs/architecture/REASON-CODE-VOCABULARIES.md` records the pair so the next reader does not have to
rediscover that they are the same condition.

### 5. The correction to `decide_exit`'s own rationale

`crates/assay-cli/src/cli/helpers.rs` justifies its change with *"`E_TRACE_MISS` and
`E_PATH_NOT_FOUND` describe one missing-trace condition and exited 1 and 2 respectively."* The
second half is right and the first half is wrong: `E_TRACE_MISS` is the coverage miss. The comment
is corrected in the same change as this ADR, because a rationale that names the wrong pair is how
the miscount in #2010 propagated.

## Consequences

### #2010's acceptance criterion, as a test that this shape can satisfy

> One condition maps to exactly one code regardless of which command reports it, and a test fails if
> a new code is added to one registry without a decision recorded for the other.

The second half is implemented, twice, and both were verified by mutation:

- `crates/assay-cli/tests/reason_code_vocabularies.rs` — a code added to any of the three
  SARIF-`ruleId`-bearing vocabularies fails until the inventory records it.
- `crates/assay-cli/tests/spec_reason_code_registry.rs` — a `ReasonCode` variant added without a
  spec registration fails, and so does a spec entry nothing emits.

The first half is **not** expressible as a test under this decision, and this ADR states that rather
than leaving it to look unimplemented. "One condition" is not a property of the code: nothing in
either registry names the condition, so no test can group two codes as describing one. What is
testable is the consequence that made the duplication harmful — that two codes for one condition
exit the same way — and `ERROR_EXIT_CLASSES` plus its tests hold that.

### What would reopen this

- A second live code appearing in both registries. One shared member is a coincidence; two is a
  vocabulary.
- The two SARIF producers being given the same `tool.driver.name`. That would make the namespaces
  genuinely shared, and the disjointness assertion in `reason_code_vocabularies.rs` is there so the
  change is a decision rather than a discovery in Code Scanning.
- A consumer that reads both artifacts and needs one string. None exists today;
  `assay_core::agentic::builder` reads only `Diagnostic.code`.

### Left open, deliberately

The 18 remediation branches in `agentic::builder` keyed on codes nothing constructs (§5.4). They are
recorded, not deleted: deleting them removes a tested remediation path whose intent is not
recoverable from the code, and §175's reasoning about consumers applies to matchers too. Whoever
knows what those branches were for should wire them up or remove them; this ADR only makes the
number visible.
