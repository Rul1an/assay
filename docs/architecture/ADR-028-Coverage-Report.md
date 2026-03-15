# ADR-028: Coverage Report (Tool & Route Completeness)

## Status
Accepted (March 2026; implemented on `main` via PRs #563, #565, #567, and #572)

## Context
Assay's governance model is deterministic policy-as-code + evidence. As tool surfaces grow (multiple sinks, alternates, protocol adapters), correctness alone is not enough: we need to detect blind spots.

Without a completeness/coverage view, teams can mistakenly believe they are governed while:
- tools are used that are not declared or classified
- sink classes exist but are not governed
- observed routes are not covered by policy intent

This ADR freezes a minimal, deterministic coverage report contract to support auditability and unknown/untaxed surface detection.

The report is informational by default (no required-check blast radius). Enforcement modes are out of scope for this ADR.

## Decision
Introduce Coverage Report v1:
- A deterministic JSON report that summarizes:
  1. tools observed in a run/session
  2. tools declared in policy/config
  3. tools unknown/undeclared
  4. taxonomy coverage (classes present vs missing)
  5. routes observed (source -> sink edges), in a bounded form

Coverage Report v1 is designed to:
- be stable/deterministic for diffing
- support SARIF/JUnit rendering later (out of scope here)
- support future mode switches (warn/enforce) without changing the schema (out of scope here)

### Definitions (frozen)
- tools_seen: tool names observed in evidence / intercepted calls during the report window.
- tools_declared: tool names explicitly declared in policy/config.
- tools_unknown: tools_seen minus tools_declared.
- tool_classes_seen: union of classes for tools_seen that have taxonomy entries.
- tool_classes_missing: tools_seen that have no taxonomy entry (empty class set).
- routes_seen: observed edges between tool classes and/or tool names, recorded deterministically.

### Severity Mapping (Informational, Frozen)
Coverage report emits findings with a severity level:
- unknown_tool:
  - severity: warning if the tool is a sink class (when known) or the name suggests sink (out of scope for v1 heuristics)
  - otherwise note
- missing_taxonomy:
  - severity: note (v1 informational only)
- uncovered_sink_class:
  - severity: warning if a sink class appears but no governance rule references that class (rule linking is out of scope for v1; placeholder field allowed)

> Note: v1 does not define heuristic sink detection. It only reports raw facts plus a minimal severity mapping based on explicit taxonomy presence.

## Consequences
### Positive
- Makes governance gaps visible and audit-friendly.
- Enables reviewers to ask "what did you not cover?" in a deterministic way.
- Directly complements Tool Taxonomy (ADR-027).

### Negative
- Requires clear definitions of the report window and observation source.
- Can generate noise if teams have many tools without taxonomy.

### Mitigations
- Keep v1 informational and deterministic.
- Provide explicit counters and lists so teams can prioritize remediation.
- Add optional allowlists later (out of scope).

## Out Of Scope (v1)
- Workflow changes or required-check additions.
- Automated enforcement based on coverage results.
- Probabilistic classification of tools.
- Full route semantics (taint tracking, dataflow labeling).
- SARIF export details (separate ADR if needed).

## Acceptance Criteria (For This Freeze Slice)
- JSON schema exists for coverage_report_v1.
- Reviewer gate enforces docs/schema-only scope and workflow-ban.
