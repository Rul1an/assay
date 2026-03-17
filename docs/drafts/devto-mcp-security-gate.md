---
title: How to add a security gate to your MCP server in 5 minutes
published: false
description: The average MCP server scores 34/100 on security. Here's how to add deterministic policy enforcement with a replayable audit trail.
tags: mcp, security, ai, opensource
---

The average MCP server scores **34/100 on security** ([source](https://dev.to/elliotllliu/we-scanned-17-popular-mcp-servers-heres-what-we-found-321c)). 100% of servers lack proper permission declarations. The OWASP MCP Top 10 lists "Lack of Audit and Telemetry" as a critical risk.

If you're building with Claude Desktop, Cursor, or Windsurf, your agent is calling tools — but do you know which ones? And can you prove what happened after the fact?

## Assay: the firewall for MCP tool calls

[Assay](https://github.com/Rul1an/assay) is an open-source CLI that wraps your MCP server with deterministic policy enforcement. Every tool call gets an explicit ALLOW or DENY with an evidence trail you can replay.

```
Agent ──► Assay ──► MCP Server
            │
            ├─ ✅ ALLOW (policy match)
            ├─ ❌ DENY  (blocked, logged)
            └─ 📋 Evidence bundle
```

No hosted backend. No API keys. MIT licensed.

## Try it in 2 minutes

**Install:**

```bash
cargo install assay-cli
```

**Create a demo workspace:**

```bash
mkdir -p /tmp/assay-demo && echo "safe content" > /tmp/assay-demo/safe.txt
```

**Wrap an MCP server with policy:**

```bash
assay mcp wrap --policy examples/mcp-quickstart/policy.yaml \
  -- npx @modelcontextprotocol/server-filesystem /tmp/assay-demo
```

**See decisions on every tool call:**

```
✅ ALLOW  read_file  path=/tmp/assay-demo/safe.txt  reason=policy_allow
❌ DENY   read_file  path=/etc/passwd                reason=path_constraint_violation
❌ DENY   exec       cmd=ls                          reason=tool_denied
```

That's it. Your MCP server now has a policy gate.

## The policy is 6 lines

```yaml
version: "1.0"
name: "my-policy"
allow: ["read_file", "list_dir"]
deny: ["exec", "shell", "write_file"]
constraints:
  - tool: "read_file"
    params:
      path:
        matches: "^/app/.*"
```

Or generate one from what your agent actually does:

```bash
assay init --from-trace trace.jsonl
```

## Add to CI in one step

```yaml
# .github/workflows/assay.yml
name: Assay Gate
on: [push, pull_request]
permissions:
  contents: read
  security-events: write
jobs:
  assay:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: Rul1an/assay-action@v2
```

PRs that violate policy get blocked. SARIF results show up in the Security tab.

## OWASP MCP Top 10 coverage

Assay covers 7 of 10 OWASP MCP risks:

| Risk | Coverage |
|------|----------|
| MCP08: Lack of Audit and Telemetry | **Complete** — evidence bundles, decision logs, replay |
| MCP02: Privilege Escalation | Strong — restrict_scope enforcement |
| MCP03: Tool Poisoning | Strong — tool signing, identity verification |
| MCP05: Command Injection | Strong — argument validation, deny rules |
| MCP07: Insufficient Auth | Strong — mandate enforcement |
| MCP10: Context Injection | Strong — redact_args enforcement |

Full mapping: [OWASP-MCP-TOP10-MAPPING.md](https://github.com/Rul1an/assay/blob/main/docs/security/OWASP-MCP-TOP10-MAPPING.md)

## What makes it different

Unlike scanners (Cisco MCP Scanner, MCPSec) that find problems, or other proxies (Intercept, mcpwall) that block calls, Assay does **enforce + evidence**:

- Every decision produces an auditable, replayable evidence bundle
- You can export, verify, diff, and lint evidence
- Compliance packs map to EU AI Act and SOC2
- [Three security experiments](https://github.com/Rul1an/assay/blob/main/docs/architecture/SYNTHESIS-TRUST-CHAIN-TRIFECTA-2026q2.md) tested 12 attack vectors with zero false positives

## Links

- GitHub: [Rul1an/assay](https://github.com/Rul1an/assay)
- Quick start: [examples/mcp-quickstart](https://github.com/Rul1an/assay/tree/main/examples/mcp-quickstart)
- CI guide: [GitHub Action](https://github.com/marketplace/actions/assay-ai-agent-security)
- Discussions: [GitHub Discussions](https://github.com/Rul1an/assay/discussions)
