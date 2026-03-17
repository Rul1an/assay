# Reddit r/LocalLLaMA Post

**Title:** Your MCP server probably scores 34/100 on security. I built an open-source firewall with replayable evidence.

---

**Body:**

A recent scan of 17 popular MCP servers found that the average scores 34/100 on security. Every single one lacked machine-readable permission declarations. OWASP now has a dedicated MCP Top 10 risk framework.

I built **Assay** to fix this. It's an open-source proxy that sits between your agent and its MCP tools.

**What it does:**

```
Agent ──► Assay ──► MCP Server
            │
            ├─ ✅ ALLOW (matches policy)
            ├─ ❌ DENY  (blocked + logged)
            └─ 📋 Evidence (replay later)
```

**Try it:**

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

**How it's different from other MCP security tools:**

- **Cisco MCP Scanner** (847 stars) scans configs. Useful, but static — tells you what *could* go wrong, not what *did*.
- **Intercept** and **mcpwall** block calls at runtime. Good, but no evidence trail.
- **Assay** does both: enforce at runtime + produce replayable evidence bundles you can export, verify, diff, and lint.

**What I'd love feedback on:**

1. Is the evidence trail useful for your workflow, or is just blocking enough?
2. Would you use this with Claude Desktop / Cursor / Windsurf?
3. Is `cargo install` a dealbreaker? Should I prioritize `brew install` or `npx`?

Covers 7/10 OWASP MCP Top 10 risks. Rust, MIT licensed, < 5ms per tool call, no hosted backend.

GitHub: https://github.com/Rul1an/assay

---

**Posting tips:**
- Engage with every comment for 2+ hours
- Answer technical questions in detail
- Be genuinely curious about feedback
- Don't be defensive about alternatives
