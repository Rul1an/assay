# ADR-032 Structurizr Workspace (2026 Q2)

> Status: Initial bounded workspace after Wave42
> Model file: [`structurizr/adr-032/workspace.dsl`](./structurizr/adr-032/workspace.dsl)

This page describes the initial Structurizr/C4 workspace for the ADR-032 line.
It is the bounded architecture-as-code layer added after the documentation maturity pass.

## Why this exists

The current ADR-032 line already had:

- a normative ADR
- a historical rollout plan
- a maintainer overview
- a building-block view
- quality scenarios

The missing piece was a machine-readable architecture model.
This workspace closes that gap without changing runtime behavior.

## What it models

The workspace models:

- system context for the Assay MCP policy stack
- container structure for runtime and evidence/replay responsibilities
- component structure for:
  - policy runtime
  - evidence and replay layer

## Why Structurizr here

As of March 2026, Structurizr’s DSL provides a text-based way to define a software architecture model based on the C4 model, and the DSL supports imported Markdown/AsciiDoc documentation via `!docs`.
It also supports inspections for missing descriptions, missing documentation, missing views, and missing relationship descriptions.

Sources:
- [Structurizr DSL](https://docs.structurizr.com/dsl)
- [Structurizr language reference](https://docs.structurizr.com/dsl/language)
- [Structurizr inspections](https://docs.structurizr.com/workspaces/inspections)
- [C4 model](https://c4model.com/)

## Workspace Contents

- DSL model: [`structurizr/adr-032/workspace.dsl`](./structurizr/adr-032/workspace.dsl)
- Embedded workspace docs: [`structurizr/adr-032/docs/`](./structurizr/adr-032/docs/)

## Intended Workflow

Use the workspace for:

1. local model inspection
2. diagram export
3. inspection/validation
4. architecture review support

Relevant Structurizr tooling/docs:
- [Structurizr local](https://docs.structurizr.com/local)
- [Structurizr documentation](https://docs.structurizr.com/ui/documentation/)
- [Structurizr component view cookbook](https://docs.structurizr.com/dsl/cookbook/component-view/)
- [Structurizr system context cookbook](https://docs.structurizr.com/dsl/cookbook/system-context-view/)

## CI Validation

The workspace is validated in CI via `.github/workflows/structurizr-validate.yml` on changes to `docs/architecture/structurizr/`.

Local validation and export:

```bash
bash scripts/structurizr-validate.sh   # validate DSL
bash scripts/structurizr-export.sh     # export Mermaid diagrams
```

Exported Mermaid views are in `structurizr/adr-032/export/`.

## Scope Discipline

This workspace is intentionally bounded.
It does not add:

- automatic ADR import via `!adrs`
- generated static-site publishing of Structurizr exports
- code-discovered components via `!components`

Those are possible later as separate bounded follow-ups.

## Relationship to the Canonical Docs

- [ADR-032](./ADR-032-MCP-Policy-Obligations-and-Evidence-v2.md): product meaning
- [ADR-032 Implementation Overview](./OVERVIEW-ADR-032-MCP-POLICY-STACK-2026q2.md): current architecture explanation
- [ADR-032 Building Block View](./BUILDING-BLOCKS-ADR-032-MCP-POLICY-STACK-2026q2.md): structural decomposition
- [ADR-032 Quality Scenarios](./QUALITY-SCENARIOS-ADR-032-MCP-POLICY-STACK-2026q2.md): quality attributes
- [ADR-032 Execution Plan](./PLAN-ADR-032-MCP-POLICY-ENFORCEMENT-2026q2.md): historical rollout
