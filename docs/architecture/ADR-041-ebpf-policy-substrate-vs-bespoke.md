# ADR-041: eBPF and Policy, Substrate-versus-Bespoke

## Status
Proposed (June 2026) — decision recorded; default posture set.

Depends on ADR-034 (contract seam); read with `docs/reference/runner/extraction-roadmap.md`.

## Context

Assay's eBPF security stack (`assay-ebpf`, `assay-monitor`, `assay-policy`) and its
two-tier policy compilation are entirely bespoke; there is zero interop code with
Falco, Tetragon, or bpfman, or with OPA or Cedar. Maintaining a parallel kernel stack
carries an operational cost, including self-hosted runner stability. As of 2026 the
niche has named occupants: Tetragon 1.4 (February 2026) is production-ready with
inline enforcement, and AgentSight and ActPlane already do independent eBPF
observation and enforcement below the agent harness. The incumbents (Falco, Tetragon,
bpfman for eBPF; OPA, Cedar for policy) are mature.

## Decision

Own only the agent-semantic mapping: kernel event to MCP tool call to policy-as-code
to claim-class. That layer is Assay's distinct contribution and is not provided by
raw kernel observation or by a general policy engine. Default posture, set now:

- Do not out-build Falco, Tetragon, or OPA. Prefer interop over a parallel
  general-purpose product.
- Evaluate running the agent-semantic layer on Tetragon 1.4 enforcement plus bpfman
  management rather than the parallel `assay-ebpf` stack.
- For generic rules, prefer compiling to or interoperating with Cedar or OPA; keep
  the bespoke language only for the agent-specific parts (tool sequences, argument
  validation, claim-class) those engines do not express natively.
- Staying bespoke for a given surface is acceptable only when justified in writing.

## Consequences

- Either path keeps Assay's distinct layer and avoids competing with mature
  incumbents and the new named neighbours.
- The substrate decision and the runner extraction decision are the same fork seen
  from two documents; they move together.

## Best-practice basis (2026)

- Falco / Tetragon / bpfman for eBPF, OPA / Cedar for policy are the incumbents;
  AgentSight and ActPlane occupy the agent-eBPF niche; the defensible position is the
  agent-semantic layer above kernel ground truth.

## Non-claims

- Assay's eBPF layer does not replace Falco or Tetragon and inherits the limits
  documented for kernel-level AI-agent enforcement (what it catches and what it
  misses).

## References

- `docs/reference/runner/extraction-roadmap.md`
- ADR-034 (contract seam), ADR-037 (runner standalone boundary)
