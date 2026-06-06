# ADR-040: Public Inspect Scorer for Claim Support

## Status
Proposed (June 2026) — depends on the sandbox evidence slice (ADR-035).

Depends on ADR-034 (contract seam).

## Context

Assay ingests structured outputs from promptfoo, mastra, pydantic, livekit,
openfeature, and cyclonedx through a receipt-importer pattern. The one high-value
eval host not yet wired is Inspect (UK AISI and Meridian). As of 2026 Inspect runs
arbitrary external agents (Claude Code, Codex CLI, Gemini CLI) as agents-under-test,
packages scorers as standard Python packages, and registers community evals through a
`/register/` folder `.yaml` submission pointing to external repositories.

## Decision

Ship the claim-support scorer as a standard Python package and register it through the
`/register/` flow, not as a fork or a core-repo change. Be a scorer inside Inspect; do
not build a competing eval harness. The scorer and any other consumer share the same
claim-class contract (ADR-034); the vocabulary is not forked. Because Inspect can
drive a coding agent as the agent-under-test, the scorer grades a coding-agent run
observed by `assay sandbox` (ADR-035), giving one end-to-end demo.

## Implementation slice

Lands as a Python package after the sandbox evidence record is consumable as the
scorer's observed-evidence source. This ADR records the decision and the integration
shape.

## Consequences

- Inspect users score claim-support without leaving Inspect, and can score
  coding-agent runs end to end.
- Adds a Python package to publish and keep in step with the Inspect scorer API and
  the register flow.

## Best-practice basis (2026)

- Inspect is the de-facto safety-eval framework; integrate as a first-class scorer via
  `/register/`. Inspect natively drives Claude Code / Codex CLI / Gemini CLI.

## References

- ADR-034 (contract seam), ADR-035 (sandbox evidence)
