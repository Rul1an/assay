### Security & Hardening
*   **Zero Conf Trust**: `assay migrate` ensures all policies are inlined.
*   **Cache Key V2**: Cache keys now include trace fingerprints (ADR-005).
*   **Trace Security**: Enforced V2 Schema for trace ingest.
*   **Strict Mode**: `assay run --strict` forces deterministic replay.

### Features
*   **Auto-Migration**: `assay migrate` automatically upgrades `v0` config.
*   **Sequence DSL**: New `rules` based syntax (`require`, `before`, `blocklist`).
*   **Legacy Compat**: `MCP_CONFIG_LEGACY=1` support.

### Internals
*   **Golden Harness**: E2E regression testing for CLI.
*   **Python SDK Cleanup**: Repo hygiene.

> **Full Changelog**: https://github.com/Rul1an/assay/compare/v0.5.0...v0.8.0-rc.1
