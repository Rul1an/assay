# C1 Mapping: OWASP Agentic ASI01 / ASI03 / ASI05

This document records the strongest assurance the current Assay pack engine can
honestly prove for a bounded OWASP Agentic probe set:

- `ASI01` Agent Goal Hijack
- `ASI03` Identity & Privilege Abuse
- `ASI05` Unexpected Code Execution

`C1` is a feasibility wave, not a user-facing OWASP baseline pack. It answers a
single question: what can engine `1.1` and the current evidence flows prove
without overclaiming?

## Evidence And Engine Constraints

- Engine `1.1` executes `event_count`, `event_pairs`, `event_field_present`,
  `event_type_exists`, `manifest_field`, `json_path_exists`, and a narrow
  conditional-presence subset.
- Unsupported checks, including conditional shapes outside the typed `v1.1`
  subset, are skipped for `security` packs. That behavior remains a release
  risk for future packs and is covered by a mandatory test in
  [owasp_agentic_c1_mapping.rs](../../crates/assay-evidence/tests/owasp_agentic_c1_mapping.rs).
- `event_types` now participates in executable event scoping. Rules with
  `event_types` evaluate only against matching events.
- A `signal gap` in this document is not based on repo search alone. It must be
  backed by a fixture or evidence-flow probe that fails to observe the required
  signal.

## Assurance Rubric

`C1` freezes one maximum currently provable level per candidate rule.

| Level | Meaning |
| --- | --- |
| `Presence` | Evidence that an event type exists. |
| `Field Presence` | Evidence that specific fields exist on at least one event. |
| `Lifecycle` | Evidence that start/finish or similar pairs exist. |
| `Linkage` | Evidence that one artifact or event correctly references another. |
| `Temporal` | Evidence that timing or validity windows are enforced. |
| `Causal/Behavioral` | Evidence of suspicious sequences, goal drift, or behavioral anomalies. |

## No-Overclaim Rule For C2

A candidate OWASP rule may only be promoted into `C2` if the shipped wording
matches the strongest machine-provable assurance level recorded here. If `C1`
only proves control evidence or field presence, `C2` must not describe the rule
as exploit detection, hijack detection, or privilege abuse prevention.

## ASI01 Agent Goal Hijack

For `ASI01`, the current engine can only prove that goal-governance control
evidence exists. It cannot prove actual goal hijack detection, deceptive tool
output detection, or multi-step drift analysis.

| Candidate Rule | Candidate Check | Evidence Signals | Target Assurance | Max Provable Level | Outcome |
| --- | --- | --- | --- | --- | --- |
| `A1-001` Decision evidence exists for governed actions | `event_type_exists(pattern=assay.tool.decision)` | `assay.tool.decision` | `Presence` | `Presence` | `yaml-only` |
| `A1-002` Decision evidence includes governance rationale fields | `event_field_present(paths_any_of=/data/reason_code,/data/policy_deny,/data/fail_closed_deny,/data/approval_state)` | `assay.tool.decision` with deny or approval context | `Field Presence` | `Field Presence` | `yaml-only` |

Interpretation:
- `A1-001` is too weak to ship on its own.
- `A1-002` can only be described as control evidence for goal governance. It is
  not goal hijack detection.

## ASI03 Identity & Privilege Abuse

For `ASI03`, the current engine can prove authorization-context presence, a
narrow conditional mandate-context requirement on allow decisions, and explicit
delegation-context visibility for supported tool-call flows. It still cannot
prove strong event-to-event mandate reference integrity, delegation-chain
completeness or integrity, inherited-scope correctness, or temporal
re-authorization semantics.

| Candidate Rule | Candidate Check | Evidence Signals | Target Assurance | Max Provable Level | Outcome |
| --- | --- | --- | --- | --- | --- |
| `A3-001` Authorization context fields exist on decisions | `event_field_present(paths_any_of=/data/principal,/data/approval_state,/data/mandate_id)` | `assay.tool.decision` authz fields | `Field Presence` | `Field Presence` | `yaml-only` |
| `A3-002` Allow decisions must carry mandate context | `conditional(if decision=allow then mandate_id exists on same event)` | `assay.tool.decision`, mandate context on allow decisions | `Field Presence` | `Field Presence` | `yaml-only` |
| `A3-003` Delegation or inherited-privilege chain is visible in evidence | `event_field_present(paths_any_of=/data/delegated_from,/data/actor_chain,/data/inherited_scopes,/data/delegation_depth)` | Supported decision flows can surface explicit delegation fields on `assay.tool.decision` | `Field Presence` | `Field Presence` | `yaml-only` |

Interpretation:
- `A3-001` is a valid yaml-only control-evidence rule.
- `A3-002` is now executable because engine `1.1` supports a narrow typed
  conditional-presence shape on the same event. It still does not prove mandate
  reference integrity or broader linkage semantics.
- `A3-003` is no longer a pure signal gap. Supported tool-call flows can
  surface explicit `_meta.delegation` context as `delegated_from` and optional
  `delegation_depth` on `assay.tool.decision`. This does not prove delegation
  chain completeness, integrity, inherited-scope correctness, or temporal
  validity.

## ASI05 Unexpected Code Execution

For `ASI05`, the current engine can prove that execution evidence exists in the
current profile-derived evidence flow, but not that degraded sandbox conditions
are emitted or that execution is safely authorized.

| Candidate Rule | Candidate Check | Evidence Signals | Target Assurance | Max Provable Level | Outcome |
| --- | --- | --- | --- | --- | --- |
| `A5-001` Process execution evidence exists | `event_type_exists(pattern=assay.process.exec)` | `assay.process.exec` from profile evidence mapping | `Presence` | `Presence` | `yaml-only` |
| `A5-002` Sandbox degradation evidence exists when containment weakens | `event_type_exists(pattern=assay.sandbox.degraded)` | `assay.sandbox.degraded` from supported weaker-than-requested fallback paths | `Presence` | `Presence` | `yaml-only` |

Interpretation:
- `A5-001` can honestly claim only execution evidence presence.
- `A5-002` is no longer a pure signal gap. Supported weaker-than-requested
  containment fallback paths can now emit `assay.sandbox.degraded` while
  execution continues. Clean baseline fixtures still omit the event by design,
  and the signal does not prove sandbox correctness or execution authorization.

## C2 Go / No-Go Summary

`C2` should not ship a broad "OWASP Agentic baseline" pack from these candidate
rules yet. The honest next step is narrower:

- only rules whose wording matches the `Max Provable Level`
- no rule that depends on unsupported `conditional` behavior
- no rule that depends on signals missing from the tested evidence flow

## Candidate Summary Table

| Candidate Rule | Max Provable Level | Ship in C2? | Reason |
| --- | --- | --- | --- |
| `A1-001` | `Presence` | `No` | Event existence alone is too weak for an OWASP-facing claim. |
| `A1-002` | `Field Presence` | `Yes` | Can ship only as control evidence for goal governance fields. |
| `A3-001` | `Field Presence` | `Yes` | Can ship only as authorization-context capture evidence. |
| `A3-002` | `Field Presence` | `No` | Engine `1.1` can execute this narrow conditional-presence form, but it remains outside the current shipped subset and does not prove mandate reference integrity. |
| `A3-003` | `Field Presence` | `No` | Supported flows can now surface explicit delegation context on decision evidence, but this remains outside the current shipped subset and does not validate chain completeness or integrity. |
| `A5-001` | `Presence` | `Yes` | Can ship only as process-execution evidence presence. |
| `A5-002` | `Presence` | `No` | Supported fallback paths now emit the signal, but it remains outside the current shipped subset and only proves degraded containment while execution continued. |
