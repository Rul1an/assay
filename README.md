<p align="center">
  <h1 align="center">Assay</h1>
  <p align="center">
    <strong>The firewall for MCP tool calls.</strong>
    <br />
    Block unsafe calls. Audit every decision. Replay anything.
  </p>
  <p align="center">
    <a href="https://crates.io/crates/assay-cli"><img src="https://img.shields.io/crates/v/assay-cli.svg" alt="Crates.io"></a>
    <a href="https://github.com/Rul1an/assay/actions/workflows/ci.yml"><img src="https://github.com/Rul1an/assay/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
    <a href="https://github.com/Rul1an/assay/blob/main/LICENSE"><img src="https://img.shields.io/crates/l/assay-core.svg" alt="License"></a>
  </p>
  <p align="center">
    <a href="examples/mcp-quickstart/">Quick Start</a> ·
    <a href="docs/guides/github-action.md">CI Guide</a> ·
    <a href="docs/architecture/SYNTHESIS-TRUST-CHAIN-TRIFECTA-2026q2.md">Security Research</a> ·
    <a href="https://github.com/Rul1an/assay/discussions">Discussions</a>
  </p>
</p>

---

Assay wraps your MCP server with deterministic policy enforcement. Every tool call gets an explicit **ALLOW** or **DENY** with a replayable evidence trail.

No hosted backend. No probabilistic filtering. No API keys for basic use.

## See It Work

```bash
cargo install assay-cli
```

```bash
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

Two commands. Immediate feedback. Your MCP server now has a policy gate.

## How It Works

```
  Agent ──► Assay proxy ──► MCP Server
               │
               ├─ ALLOW / DENY (deterministic)
               ├─ Evidence trail (auditable)
               └─ Replay bundle (reproducible)
```

Assay intercepts every tool call on the MCP transport, evaluates it against your policy, and emits a decision with evidence. Blocked calls never reach the server.

## Use Cases

**You're building with Claude Desktop, Cursor, or Windsurf** and you want to know exactly which tools your agent calls — and stop the ones you didn't expect.

**Your team ships MCP-enabled agents** and you need a CI gate that catches tool-call regressions before they reach production.

**You need an audit trail** for compliance, debugging, or security review — and you need it to be deterministic, not sampled.

## Get Started in 3 Ways

### Wrap locally

```bash
assay mcp wrap --policy policy.yaml -- your-mcp-server
```

### Gate in CI

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

### Generate from behavior

Don't write policy from scratch. Record what your agent does, then lock it down:

```bash
assay init --from-trace trace.jsonl
```

## Policy in 6 Lines

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

## Why Assay

| | |
|---|---|
| **Deterministic** | Same input, same decision, every time. No probabilistic filtering. |
| **MCP-native** | Built for the Model Context Protocol tool-call path, not bolted on. |
| **Evidence trail** | Every decision produces an auditable, replayable bundle. |
| **Offline-first** | No hosted backend. Your policies and traces never leave your machine. |
| **Fast** | < 5ms overhead per tool call in published benchmarks. |
| **Battle-tested** | [Three security experiments](docs/architecture/SYNTHESIS-TRUST-CHAIN-TRIFECTA-2026q2.md), 12 attack vectors, zero false positives. |

## Install

```bash
cargo install assay-cli
```

Use the [GitHub Action](https://github.com/marketplace/actions/assay-ai-agent-security) in CI without installing locally.

Python: `pip install assay-it`

## Learn More

- [MCP Quickstart Example](examples/mcp-quickstart/)
- [CI Integration Guide](docs/guides/github-action.md)
- [Evidence Store Setup](docs/guides/evidence-store-aws-s3.md) (AWS S3, [Backblaze B2](docs/guides/evidence-store-backblaze-b2.md), [MinIO](docs/guides/evidence-store-minio.md))
- [Architecture](docs/architecture/index.md)
- [Roadmap](docs/ROADMAP.md)

## Contributing

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

See [CONTRIBUTING.md](CONTRIBUTING.md). Join the [discussion](https://github.com/Rul1an/assay/discussions).

## License

[MIT](LICENSE)
