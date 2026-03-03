# Fragmented IPI Compat Host

This is an experiment-only MCP stdio host for the fragmented IPI harness.

It exposes exactly:
- `read_document`
- `web_search`

Required environment:
- `COMPAT_ROOT`

Optional environment:
- `COMPAT_AUDIT_LOG`

Example live command:

```bash
COMPAT_ROOT="$PWD/scripts/ci/fixtures/exp-mcp-fragmented-ipi" \
MCP_HOST_CMD="python3 scripts/ci/exp-mcp-fragmented-ipi/compat_host/compat_host.py" \
RUN_LIVE=1 \
bash scripts/ci/test-exp-mcp-fragmented-ipi-ablation.sh
```

This host is experiment infrastructure only. It is not a product MCP server.
