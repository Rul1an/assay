# Guides

Technical implementation guides and architectural patterns for Assay.

## Operational Patterns

*   [**Gateway Pattern**](gateway-pattern.md): Reference architecture for deploying Assay as a runtime policy enforcement point (PEP) or sidecar. Use this for production traffic filtering.

## Integration Guides

*   [**CI/CD Integration**](../getting-started/ci-integration.md): Configuring Assay in GitHub Actions, GitLab CI, and other pipelines.
*   [**Self-Correction**](../mcp/self-correction.md): Implementing runtime policy checks within MCP clients.
*   [**OpenTelemetry & Langfuse**](otel-langfuse.md): Reuse existing traces for deterministic replay, policy gates, and evidence bundles.
