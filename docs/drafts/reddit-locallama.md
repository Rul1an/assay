# Reddit r/LocalLLaMA Post

Title: I built an open-source firewall for MCP tool calls (with a replayable audit trail)

---

Body:

The average MCP server scores 34/100 on security. OWASP now has a dedicated MCP Top 10 risk framework.

I built **Assay** — it wraps your MCP server as a proxy and gives every tool call an explicit ALLOW or DENY, with a replayable evidence trail.

**Two commands to try it:**

```
cargo install assay-cli
assay mcp wrap --policy examples/mcp-quickstart/policy.yaml \
  -- npx @modelcontextprotocol/server-filesystem /tmp/demo
```

**What you see:**
```
✅ ALLOW  read_file  path=/tmp/demo/safe.txt  reason=policy_allow
❌ DENY   read_file  path=/etc/passwd          reason=path_constraint_violation
❌ DENY   exec       cmd=ls                    reason=tool_denied
```

**What makes it different from other MCP security tools:**
- Not just scanning (like Cisco MCP Scanner) — it enforces policy at runtime
- Not just blocking (like Intercept/mcpwall) — it produces evidence bundles you can replay/diff
- Covers 7/10 OWASP MCP Top 10 risks
- Tested with 12 security attack vectors, 0 false positives
- Compliance packs for EU AI Act and SOC2

Rust, MIT licensed, no hosted backend, < 5ms per tool call.

GitHub: https://github.com/Rul1an/assay

Happy to answer questions about the architecture or the security experiments.
