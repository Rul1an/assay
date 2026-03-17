# Show HN: Assay – The firewall for MCP tool calls, with replayable evidence

URL: https://github.com/Rul1an/assay

---

First comment (post immediately after submission):

I built Assay because the average MCP server scores 34/100 on security [1], and OWASP now lists "Lack of Audit and Telemetry" as a top MCP risk.

Assay wraps your MCP server as a proxy. Every tool call gets an explicit ALLOW or DENY based on a YAML policy, and every decision produces a replayable evidence bundle.

Two commands to try it:

    cargo install assay-cli
    assay mcp wrap --policy examples/mcp-quickstart/policy.yaml -- npx @modelcontextprotocol/server-filesystem /tmp/demo

What makes it different from other MCP security tools (Intercept, mcpwall, Cisco MCP Scanner): it doesn't just block — it produces evidence you can export, verify, diff, and replay. Compliance packs map to EU AI Act and SOC2.

Rust, MIT licensed, no hosted backend. Single-digit ms overhead per tool call.

Covers 7/10 OWASP MCP Top 10 risks: https://github.com/Rul1an/assay/blob/main/docs/security/OWASP-MCP-TOP10-MAPPING.md

[1] https://dev.to/elliotllliu/we-scanned-17-popular-mcp-servers-heres-what-we-found-321c

---

Best timing: Tuesday-Thursday, 8-10 AM EST
