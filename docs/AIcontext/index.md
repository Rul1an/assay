# AI Context Documentation

> **Version**: 2.8.0 (January 2026)

This directory contains comprehensive documentation designed specifically for AI agents (LLMs) to understand and work with the Assay codebase. These documents follow best practices for AI context management as of January 2026.

## Purpose

These documents provide:
- **Structured context** for AI agents to understand the codebase
- **User flow mappings** showing how different actors interact with the system
- **Dependency graphs** showing crate relationships and interfaces
- **Architecture diagrams** in Mermaid format for visual understanding
- **Entry point documentation** for all ways to interact with Assay

## Document Structure

| Document | Purpose |
|----------|---------|
| [Codebase Overview](codebase-overview.md) | High-level description of what Assay is, its architecture, and core components |
| [User Flows](user-flows.md) | Complete user journeys from different perspectives (developer, CI, runtime) |
| [Interdependencies](interdependencies.md) | Crate dependencies, interfaces, and data flow between components |
| [Architecture Diagrams](architecture-diagrams.md) | Visual representations of system architecture, data flows, and component relationships |
| [Entry Points](entry-points.md) | All ways to interact with Assay (CLI commands, Python SDK, MCP server) |
| [Code Map](code-map.md) | Detailed mapping of important files, modules, and their responsibilities |

## Best Practices Applied

This documentation follows 2026 best practices for AI codebase understanding:

1. **Focused Context**: Each document covers a specific aspect to avoid context overflow
2. **Structured Format**: Consistent markdown with clear sections and hierarchies
3. **Visual Aids**: Mermaid diagrams for complex relationships and flows
4. **Entry Point Clarity**: Clear documentation of all interaction points
5. **Dependency Mapping**: Explicit documentation of how components connect
6. **User-Centric**: Flows organized by user type and use case

## Quick Reference

### For Understanding the System
- Start with [Codebase Overview](codebase-overview.md) for high-level understanding
- Review [Architecture Diagrams](architecture-diagrams.md) for visual context
- Check [Interdependencies](interdependencies.md) to understand component relationships

### For Implementing Features
- Review [Entry Points](entry-points.md) to find where to add new functionality
- Check [Code Map](code-map.md) to locate relevant files
- Understand [User Flows](user-flows.md) to see how features are used

### For Debugging
- Use [User Flows](user-flows.md) to trace execution paths
- Check [Interdependencies](interdependencies.md) to understand data flow
- Review [Code Map](code-map.md) to find relevant modules

## Maintenance

These documents should be updated when:
- New crates or major modules are added
- User flows change significantly
- New entry points are added (CLI commands, SDK methods, etc.)
- Architecture changes (new tiers, components, etc.)

## Related Documentation

- [Architecture ADRs](../architecture/) - Architecture Decision Records
- [Core Concepts](../concepts/) - User-facing concept documentation
- [CLI Reference](../reference/cli/) - Detailed CLI command documentation
- [Python SDK](../python-sdk/) - Python SDK documentation
