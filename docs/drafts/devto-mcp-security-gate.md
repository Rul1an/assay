---
title: Your MCP server probably scores 34/100 on security. Here's the fix.
published: false
description: How to add a policy gate and audit trail to any MCP server in under 5 minutes.
tags: mcp, security, ai, opensource
cover_image:
---

A recent scan of 17 popular MCP servers found an average security score of **34 out of 100**. Every single server lacked machine-readable permission declarations. Nearly a third scored as high risk — including official servers from Anthropic, AWS, and Cloudflare.

Meanwhile, MCP adoption is accelerating: 97 million SDK downloads per month, 10,000+ servers in public registries, and OWASP just published a dedicated [MCP Top 10](https://owasp.org/www-project-mcp-top-10/) risk framework.

The gap between adoption and security is widening. Here's one way to close it.

## The problem in one question

Your agent calls `read_file`, `exec`, `web_search` through MCP. But:

- Do you know **which** tools it called?
- Can you **prove** what happened?
- Can you **replay** a session to debug it?

If the answer is no, you have an MCP08 problem — "Lack of Audit and Telemetry," listed as a critical OWASP MCP risk.

## The fix: a policy gate with an evidence trail

[Assay](https://github.com/Rul1an/assay) is an open-source proxy that sits between your agent and its MCP server:

```
Agent ──► Assay ──► MCP Server
            │
            ├─ ✅ ALLOW (matches policy)
            ├─ ❌ DENY  (blocked + logged)
            └─ 📋 Evidence bundle (replay later)
```

Every tool call gets an explicit decision. Every decision gets evidence. Nothing hidden, nothing sampled.

## Try it now

```bash
cargo install assay-cli

mkdir -p /tmp/assay-demo && echo "safe content" > /tmp/assay-demo/safe.txt

assay mcp wrap --policy examples/mcp-quickstart/policy.yaml \
  -- npx @modelcontextprotocol/server-filesystem /tmp/assay-demo
```

What you see:

```
✅ ALLOW  read_file  path=/tmp/assay-demo/safe.txt  reason=policy_allow
❌ DENY   read_file  path=/etc/passwd                reason=path_constraint_violation
❌ DENY   exec       cmd=ls                          reason=tool_denied
```

Three commands. Immediate feedback. Your MCP server has a policy gate.

## The policy is simple

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

Don't want to write one from scratch? Generate it from what your agent already does:

```bash
assay init --from-trace trace.jsonl
```

## Add to CI

```yaml
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

PRs that violate policy get blocked. SARIF results appear in GitHub's Security tab.

## Why not just a scanner?

Tools like [Cisco MCP Scanner](https://github.com/cisco-ai-defense/mcp-scanner) (847 stars) scan your MCP configs for vulnerabilities. That's useful — but it's static. It tells you what _could_ go wrong, not what _did_ go wrong.

Other proxies like [Intercept](https://github.com/policylayer/intercept) and [mcpwall](https://github.com/behrensd/mcp-firewall) block calls at runtime. Good — but they don't produce evidence you can audit or replay.

Assay does both: **enforce + evidence**.

- Every decision produces a tamper-evident bundle
- You can `export`, `verify`, `diff`, and `lint` evidence
- Compliance packs map to EU AI Act and SOC2
- Tested against [12 security attack vectors](https://github.com/Rul1an/assay/blob/main/docs/architecture/SYNTHESIS-TRUST-CHAIN-TRIFECTA-2026q2.md) with zero false positives

## OWASP MCP Top 10 coverage

Assay addresses 7 of 10 OWASP MCP risks. The strongest alignment:

| Risk | What Assay does |
|------|----------------|
| **MCP08: Lack of Audit** | Evidence bundles + decision logs + replay |
| MCP02: Privilege Escalation | restrict_scope enforcement |
| MCP03: Tool Poisoning | Tool signing + identity verification |
| MCP05: Command Injection | Argument validation + deny rules |
| MCP07: Insufficient Auth | Mandate enforcement + approval gates |

[Full OWASP mapping →](https://github.com/Rul1an/assay/blob/main/docs/security/OWASP-MCP-TOP10-MAPPING.md)

## Get started

- **GitHub**: [Rul1an/assay](https://github.com/Rul1an/assay)
- **Quick start**: [examples/mcp-quickstart](https://github.com/Rul1an/assay/tree/main/examples/mcp-quickstart)
- **CI guide**: [GitHub Action](https://github.com/marketplace/actions/assay-ai-agent-security)
- **Discuss**: [GitHub Discussions](https://github.com/Rul1an/assay/discussions)

Rust, MIT licensed, offline-first. No hosted backend, no API keys for basic use.
