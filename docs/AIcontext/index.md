# AI Context Documentation

This directory helps agents navigate the Assay codebase. It is not a release
support matrix or an execution ledger. Start with the checked-in
[agent contract](../../AGENTS.md), then the
[evidence-first scope](../architecture/ADR-042-evidence-first-positioning.md) and
[integrity boundaries](../architecture/ADR-043-evidence-chain-integrity-invariants.md).

Assay's contract is the open evidence profile for privileged MCP tool actions.
Its enforcing proxy is a reference producer. Policy decisions, observations and
provider outcomes remain distinct; installation or a successful command does not
prove that an external action occurred.

## Quick Start for AI Agents

**Most Important Files to Read First:**
1. [Product Support](../reference/product-support.md) - Capability status and explicit limits
2. [Agent Golden Path](../guides/agent-golden-path.md) - Install-to-evidence journey
3. [Quick Reference](quick-reference.md) - Command cheat sheet and common patterns
4. [Decision Trees](decision-trees.md) - Which command/approach to use when
5. [Codebase Overview](codebase-overview.md) - What Assay is and how it works

## Purpose

These documents provide:
- **Structured context** for AI agents to understand the codebase
- **User flow mappings** showing how different actors interact with the system
- **Dependency graphs** showing crate relationships and interfaces
- **Architecture diagrams** in Mermaid format for visual understanding
- **Entry point documentation** for all ways to interact with Assay
- **Decision trees** for choosing the right approach
- **CI infrastructure** documentation for self-hosted runners and optimization

## Document Structure

| Document | Purpose | Priority |
|----------|---------|----------|
| [Quick Reference](quick-reference.md) | **NEW** Command cheat sheet, common patterns, exit codes | ⭐ High |
| [Decision Trees](decision-trees.md) | **NEW** When to use which command/approach | ⭐ High |
| [Codebase Overview](codebase-overview.md) | High-level description of what Assay is, its architecture, and core components | ⭐ High |
| [User Flows](user-flows.md) | Complete user journeys from different perspectives (developer, CI, runtime) | Medium |
| [Interdependencies](interdependencies.md) | Crate dependencies, interfaces, and data flow between components | Medium |
| [Architecture Diagrams](architecture-diagrams.md) | Visual representations of system architecture, data flows, and component relationships | Medium |
| [Entry Points](entry-points.md) | All ways to interact with Assay (CLI commands, Python SDK, MCP server) | Medium |
| [Code Map](code-map.md) | Detailed mapping of important files, modules, and their responsibilities | Low |
| [CI Infrastructure](ci-infrastructure.md) | **NEW** Self-hosted runner, health checks, CI optimization | Low |
| [Run Output](run-output.md) | **NEW** run.json / summary.json contract: seeds, judge_metrics, reason_code (PR gate) | Medium |

## Capability And Release Truth

Use [Product Support](../reference/product-support.md) for capability status,
[installation](../getting-started/installation.md) for the published install pin,
and [Cargo.toml](../../Cargo.toml) for the workspace version. A release-preparation
tree can legitimately lead the published version; do not duplicate those values here.

The [editor recipe](../guides/editor-mcp-recipe.md) distinguishes the five local
MCP review tools from wrapping a target server for enforcement. Plain stdio mode
does not implement transport authentication. Host discovery and authenticated
model use require their own evidence; static manifests do not prove either.

## Best Practices Applied

This documentation follows 2026 best practices for AI codebase understanding:

1. **Focused Context**: Each document covers a specific aspect to avoid context overflow
2. **Structured Format**: Consistent markdown with clear sections and hierarchies
3. **Visual Aids**: Mermaid diagrams for complex relationships and flows
4. **Entry Point Clarity**: Clear documentation of all interaction points
5. **Dependency Mapping**: Explicit documentation of how components connect
6. **User-Centric**: Flows organized by user type and use case
7. **Decision Support**: Decision trees for common choices
8. **LLM-Optimized**: Tables, structured data, and clear naming

## Quick Reference

### For Understanding the System
- Start with [Quick Reference](quick-reference.md) for immediate context
- Review [Codebase Overview](codebase-overview.md) for high-level understanding
- Check [Architecture Diagrams](architecture-diagrams.md) for visual context
- Check [Interdependencies](interdependencies.md) to understand component relationships

### For Implementing Features
- Use [Decision Trees](decision-trees.md) to find the right approach
- Review [Entry Points](entry-points.md) to find where to add new functionality
- Check [Code Map](code-map.md) to locate relevant files
- Understand [User Flows](user-flows.md) to see how features are used

### For Debugging
- Use [User Flows](user-flows.md) to trace execution paths
- Check [Interdependencies](interdependencies.md) to understand data flow
- Review [Code Map](code-map.md) to find relevant modules
- Check [Quick Reference](quick-reference.md) for exit codes and error patterns

### For CI/CD Work
- Review [CI Infrastructure](ci-infrastructure.md) for runner setup
- Check [User Flows](user-flows.md) Flow 2 for CI integration
- See [Entry Points](entry-points.md) for GitHub Action configuration

## Command Results

Use the selected command's [CLI reference](../reference/cli/index.md) and typed
output contract. Exit zero is not a universal policy-allow or enforcement result:
`doctor` may report unavailable capabilities, and an MCP server can exit cleanly
after returning a request-level error. Do not apply the evaluation exit table to
every command or infer a successful provider action from process completion.

**Run output (PR #159, #160):** `run.json` and `summary.json` include `seeds` (order_seed, judge_seed as string or null), `judge_metrics`, `reason_code`, and when SARIF was truncated `sarif.omitted`. Console: `Seeds: seed_version=1 order_seed=… judge_seed=…`. See [Run Output](run-output.md).

## Maintenance

These documents should be updated when:
- New crates or major modules are added
- User flows change significantly
- New entry points are added (CLI commands, SDK methods, etc.)
- Architecture changes (new tiers, components, etc.)
- Exit codes or reason codes change
- CI infrastructure changes

## Related Documentation

- [Run Output](run-output.md) - run.json / summary.json contract (seeds, judge_metrics, reason_code)
- [Architecture ADRs](../architecture/index.md) - Architecture Decision Records
- [Core Concepts](../concepts/index.md) - User-facing concept documentation
- [CLI Reference](../reference/cli/index.md) - Detailed CLI command documentation
- [Python SDK](../python-sdk/index.md) - Python SDK documentation
- [SPEC-PR-Gate-Outputs-v1](../architecture/SPEC-PR-Gate-Outputs-v1.md) - PR gate output spec
- [DX Roadmap](../DX-ROADMAP.md) - Current DX execution plan
