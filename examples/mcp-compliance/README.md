# MCP Compliance Example

Minimal MCP server that satisfies marketplace scanners (MCP Marketplace, etc.) which look for:

- `@modelcontextprotocol/sdk` import
- Server instance
- At least one tool definition
- Transport setup (stdio or HTTP)

**Assay is a Rust-based MCP policy firewall.** The main product is `assay mcp wrap` and `assay-mcp-server`. This TypeScript file exists so repository scans pass validation.

## Run this example

```bash
cd examples/mcp-compliance
npm install
npx tsx index.ts
```

## Run the real Assay

```bash
cargo install assay-cli
assay mcp wrap --policy policy.yaml -- npx @modelcontextprotocol/server-filesystem /tmp/demo
```
