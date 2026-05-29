# MCP Tool Evidence Binding Harness Runs

This directory contains checked-in synthetic outputs for the MCP tool
evidence-binding harness.

## Starter Synthetic

`starter-synthetic/` contains the six starter scenario outputs generated
by:

```bash
python3 ../mcp_tool_binding_harness.py \
  --out-dir starter-synthetic \
  --assay-commit synthetic-starter-output \
  --created-at 2026-05-29T00:00:00Z
```

The harness test suite regenerates this directory and compares it
byte-for-byte against the committed files. These outputs are synthetic
review artifacts only: they do not contact live MCP servers, deploy MCP
tunnels, detect poisoned tools, classify maliciousness, or promote a
receipt family.
