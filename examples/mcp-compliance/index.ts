/**
 * Minimal MCP server for marketplace compliance.
 *
 * Assay is a Rust-based MCP policy firewall (assay mcp wrap, assay-mcp-server).
 * This file satisfies scanners that look for: @modelcontextprotocol/sdk import,
 * server instance, at least one tool definition, and stdio transport.
 *
 * To run the real Assay: cargo install assay-cli && assay mcp wrap --policy policy.yaml -- your-mcp-server
 */
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";

const server = new McpServer({
  name: "assay",
  version: "1.0.0",
});

server.tool(
  "assay_info",
  "Returns Assay MCP policy firewall info. Use assay mcp wrap to enforce policy on tool calls.",
  {},
  async () => ({
    content: [
      {
        type: "text",
        text: "Assay: MCP policy firewall. cargo install assay-cli && assay mcp wrap --policy policy.yaml -- your-mcp-server",
      },
    ],
  })
);

const transport = new StdioServerTransport();
await server.connect(transport);
