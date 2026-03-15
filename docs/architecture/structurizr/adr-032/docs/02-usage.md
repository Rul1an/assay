# Usage Notes

## Why this workspace exists

The ADR-032 docs already describe:

- product boundary
- rollout history
- building blocks
- quality scenarios

This workspace adds a machine-readable C4 model so the current shape of the stack can be inspected, exported, and validated more easily.

## Boundaries

This workspace is intentionally bounded.
It does not:

- change runtime behavior
- replace the Markdown docs as the source of truth
- introduce a control plane
- model every crate in the repository

## How it should be used

Use this workspace to:

- inspect the current system/context/container/component structure
- export views to static images or Mermaid/PlantUML for review
- run Structurizr inspections against the current model
- anchor future architecture-as-code follow-ups on a stable baseline

## Relationship to the Markdown docs

- ADR: normative decisions
- overview: current architecture explanation
- building blocks: structural decomposition
- quality scenarios: quality attributes and review checks
- Structurizr workspace: machine-readable model and diagrams
