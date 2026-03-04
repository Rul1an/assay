# ADR-027: Tool Taxonomy and Class-Based Route Policies

## Status

Proposed (March 2026, A1 freeze)

## Context

The March 2026 fragmented-IPI experiment line on `main` closed a consistent route-governance story:
- payload evasions break `wrap_only`
- tool-hopping and alternate sink labels break `wrap_only`
- sink-failure pressure further degrades `wrap_only`
- cross-session delayed sink attempts still require stateful route memory

The common weakness behind these failures is tool-name brittleness. Policies written only against explicit tool names do not generalize well when the same route semantics appear under alternate names or adjacent surfaces.

Assay's product wedge is deterministic governance on tool routes. To make that wedge reusable outside experiment harnesses, the policy model needs a first-class way to describe tools as capabilities and route roles rather than only as raw names.

## Decision

We introduce a tool taxonomy contract with class-based route policy matching.

### v1 taxonomy primitives

Policies may classify tools using one or more string labels, for example:
- `source:local`
- `source:sensitive`
- `sink:network`
- `sink:external`
- `store:persistent`
- `exec:command`

The taxonomy is descriptive, not magical. Classes are explicit policy/config metadata supplied by the user or pack, not inferred dynamically by an LLM or hidden heuristic.

### v1 matching model

Route rules may match on:
- explicit tool name
- one or more tool classes

Normative intent:
- tool-name matching remains supported for backward compatibility
- class matching is the preferred generalization layer
- a rule may combine both forms when needed

### v1 evidence and decision contract

When a class-based rule matches, decision/evidence output must preserve the class match explicitly.

Required reporting fields for matched class decisions:
- `matched_tool_name`
- `matched_tool_classes`
- `matched_route_rule_id`
- `reason_code`

This keeps class-based enforcement auditable and replayable.

## Minimal Schema Contract

The basis of the taxonomy contract is a deterministic mapping:
- `tool_classes`
  - key: tool name
  - value: non-empty ordered list of class strings

Optional route rules may then reference:
- `match_tool`
- `match_any_class`
- `match_all_classes`

The A1 schema only freezes the shape and required fields. Runtime semantics are implemented in B1.

## Examples

### Tool classification

```json
{
  "tool_classes": {
    "read_document": ["source:local", "source:sensitive"],
    "web_search": ["sink:network"],
    "web_search_alt": ["sink:network", "sink:external"]
  }
}
```

### Route rule intent

```yaml
route_rules:
  - id: no_sink_after_sensitive_source
    after:
      match_any_class: ["source:sensitive"]
    deny:
      match_any_class: ["sink:network", "sink:external"]
```

## Non-Goals

This ADR does not introduce:
- dynamic class inference from prompts or payload contents
- LLM-judged route classification
- automatic discovery of unknown tools as a hard enforcement source
- workflow changes or CI rollout changes
- removal of existing name-based policy matching

## A/B/C Execution Plan

### PR-A1 (freeze)
- freeze taxonomy semantics
- freeze schema basis
- freeze decision/evidence reporting requirement for matched classes
- no runtime changes

### PR-B1 (implement)
- add class matching to policy evaluation
- add fixtures and regression tests for tool-hopping via class rules
- keep name-based policies backward compatible

### PR-C1 (closure)
- add migration/runbook docs
- add checklist and review pack
- close the rollout loop with reviewer gates

## Acceptance Criteria

- ADR defines class taxonomy semantics and bounded non-goals
- ADR freezes class-based route matching as additive to name matching
- ADR requires decision/evidence output to report matched classes
- A1 introduces no runtime or workflow changes

## Consequences

### Positive
- reduces brittleness from raw tool-name matching
- aligns policy semantics with route governance rather than string matching
- makes tool-hopping defenses reusable beyond experiment harnesses

### Negative
- adds a new compatibility surface for policy/schema evolution
- requires clear pack/docs discipline so classes stay explicit and reviewable

### Mitigations
- freeze-first rollout
- explicit schema contract
- evidence/decision reporting for matched classes
