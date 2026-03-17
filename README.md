<p align="center">
  <h1 align="center">Assay</h1>
  <p align="center">
    <strong>The firewall for MCP tool calls — with a replayable audit trail.</strong>
  </p>
  <p align="center">
    <a href="https://crates.io/crates/assay-cli"><img src="https://img.shields.io/crates/v/assay-cli.svg" alt="Crates.io"></a>
    <a href="https://github.com/Rul1an/assay/actions/workflows/ci.yml"><img src="https://github.com/Rul1an/assay/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
    <a href="https://github.com/Rul1an/assay/blob/main/LICENSE"><img src="https://img.shields.io/crates/l/assay-core.svg" alt="License"></a>
  </p>
  <p align="center">
    <a href="#see-it-work">See It Work</a> ·
    <a href="examples/mcp-quickstart/">Quick Start</a> ·
    <a href="docs/guides/github-action.md">CI Guide</a> ·
    <a href="https://github.com/Rul1an/assay/discussions">Discussions</a>
  </p>
</p>

---

Your MCP agent calls `read_file`, `exec`, `web_search` — but should it?

Assay sits between your agent and its tools. It intercepts every MCP tool call, checks it against your policy, and blocks what shouldn't happen. Every decision produces an evidence trail you can audit, diff, and replay.

```
  Agent ──► Assay ──► MCP Server
              │
              ├─ ✅ ALLOW (policy match)
              ├─ ❌ DENY  (blocked, logged)
              └─ 📋 Evidence bundle
```

No hosted backend. No API keys. Deterministic — same input, same decision, every time.

> The average MCP server scores [34/100 on security](https://dev.to/elliotllliu/we-scanned-17-popular-mcp-servers-heres-what-we-found-321c). Assay gives you the policy gate and audit trail to fix that. Covers [7 of 10 OWASP MCP Top 10](docs/security/OWASP-MCP-TOP10-MAPPING.md) risks.

## See It Work

```bash
cargo install assay-cli

mkdir -p /tmp/assay-demo && echo "safe content" > /tmp/assay-demo/safe.txt

assay mcp wrap --policy examples/mcp-quickstart/policy.yaml \
  -- npx @modelcontextprotocol/server-filesystem /tmp/assay-demo
```

```
✅ ALLOW  read_file  path=/tmp/assay-demo/safe.txt  reason=policy_allow
✅ ALLOW  list_dir   path=/tmp/assay-demo/           reason=policy_allow
❌ DENY   read_file  path=/etc/passwd                reason=path_constraint_violation
❌ DENY   exec       cmd=ls                          reason=tool_denied
```

## Is This For Me?

**Yes, if you:**
- Build with Claude Desktop, Cursor, Windsurf, or any MCP client
- Ship agents that call tools and you need to control which ones
- Want a CI gate that catches tool-call regressions before production
- Need a deterministic audit trail, not sampled observability

**Not yet, if you:**
- Don't use MCP (Assay is MCP-native; other protocols are on the roadmap)
- Need a hosted dashboard (Assay is CLI-first and offline)

## Policy Is Simple

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

Or don't write one — generate it from what your agent actually does:

```bash
assay init --from-trace trace.jsonl
```

## Add to CI

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

## Beyond MCP: Protocol Adapters

Assay already ships adapters for emerging agent protocols:

| Protocol | Adapter | What it maps |
|----------|---------|-------------|
| **ACP** (OpenAI/Stripe) | `assay-adapter-acp` | Checkout events, payment intents, tool calls |
| **A2A** (Google) | `assay-adapter-a2a` | Agent capabilities, task delegation, artifacts |
| **UCP** (Google/Shopify) | `assay-adapter-ucp` | Discover/buy/post-purchase state transitions |

Each adapter translates protocol-specific events into Assay's canonical evidence format. Same policy engine, same evidence trail — regardless of which protocol your agent speaks.

The agent protocol landscape is fragmenting (ACP, A2A, UCP, AP2, x402). Assay's bet: **governance is protocol-agnostic.** The evidence and policy layer stays the same even as protocols come and go.

## Why Assay

| | |
|---|---|
| **Deterministic** | Same input, same decision, every time. Not probabilistic. |
| **MCP-native** | Built for MCP tool calls. Adapters for ACP, A2A, UCP. |
| **Evidence trail** | Every decision is auditable, diffable, replayable. |
| **Offline-first** | No backend, no API keys. Runs on your machine. |
| **Fast** | < 5ms per tool call. |
| **Tested** | [3 security experiments](docs/architecture/SYNTHESIS-TRUST-CHAIN-TRIFECTA-2026q2.md), 12 attack vectors, 0 false positives. |

## Install

```bash
cargo install assay-cli
```

In CI: use the [GitHub Action](https://github.com/marketplace/actions/assay-ai-agent-security) directly.

Python SDK: `pip install assay-it`

## Learn More

- [MCP Quickstart](examples/mcp-quickstart/) — full walkthrough with a filesystem server
- [CI Guide](docs/guides/github-action.md) — GitHub Action setup
- [OWASP MCP Top 10 Mapping](docs/security/OWASP-MCP-TOP10-MAPPING.md) — how Assay addresses each risk
- [Evidence Store](docs/guides/evidence-store-aws-s3.md) — push bundles to S3, B2, or MinIO
- [Security Experiments](docs/architecture/SYNTHESIS-TRUST-CHAIN-TRIFECTA-2026q2.md) — 12 vectors, 0 false positives

## Contributing

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

See [CONTRIBUTING.md](CONTRIBUTING.md). Join the [discussion](https://github.com/Rul1an/assay/discussions).

## License

[MIT](LICENSE)
