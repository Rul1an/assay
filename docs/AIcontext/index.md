# AI Context Documentation

> **Version**: 2.12.0 (January 2026)
> **Last Updated**: 2026-01-31
> **SOTA Status**: Bleeding Edge (Judge Reliability, MCP Auth, OTel GenAI, Replay Bundle)

This directory contains comprehensive documentation designed specifically for AI agents (LLMs) to understand and work with the Assay codebase. These documents follow best practices for AI context management as of January 2026.

## Quick Start for AI Agents

**Most Important Files to Read First:**
1. [Quick Reference](quick-reference.md) - Command cheat sheet and common patterns
2. [Decision Trees](decision-trees.md) - Which command/approach to use when
3. [Codebase Overview](codebase-overview.md) - What Assay is and how it works

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

## SOTA Features (January 2026)

| Feature | Status | Description |
|---------|--------|-------------|
| **Judge Reliability** | ✅ Implemented | Randomized order, borderline band, 2-of-3 rerun, per-suite policies |
| **MCP Auth Hardening** | 🔄 P1 | RFC 8707, alg/typ/crit, JWKS rotation, DPoP (optional) |
| **OTel GenAI** | 🔄 P1 | Semconv versioning, low-cardinality metrics, composable redaction |
| **Replay Bundle** | 🔄 P1 | Toolchain capture, deterministic seeds, scrubbed cassettes |
| **CI Optimization** | ✅ Implemented | Skip matrix tests for pure dep bumps, auto-cancel superseded runs |
| **Self-Healing Runner** | ✅ Implemented | Health check, cache auto-heal, stale job cleanup |

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

## Exit Code Quick Reference

| Code | Meaning | Common Causes |
|------|---------|---------------|
| 0 | Success | All tests pass |
| 1 | Test failure | Policy violation, metric failure |
| 2 | Config error | Invalid YAML, missing file, parse error |
| 3 | Infra error | Judge unavailable, rate limit, timeout |

## Maintenance

These documents should be updated when:
- New crates or major modules are added
- User flows change significantly
- New entry points are added (CLI commands, SDK methods, etc.)
- Architecture changes (new tiers, components, etc.)
- Exit codes or reason codes change
- CI infrastructure changes

## Related Documentation

- [Architecture ADRs](../architecture/) - Architecture Decision Records
- [Core Concepts](../concepts/) - User-facing concept documentation
- [CLI Reference](../reference/cli/) - Detailed CLI command documentation
- [Python SDK](../python-sdk/) - Python SDK documentation
- [DX Implementation Plan](../DX-IMPLEMENTATION-PLAN.md) - Current DX roadmap
