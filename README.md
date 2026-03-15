# Assay

[![Crates.io](https://img.shields.io/crates/v/assay-cli.svg)](https://crates.io/crates/assay-cli)
[![CI](https://github.com/Rul1an/assay/actions/workflows/ci.yml/badge.svg)](https://github.com/Rul1an/assay/actions/workflows/ci.yml)
[![License](https://img.shields.io/crates/l/assay-core.svg)](https://github.com/Rul1an/assay/blob/main/LICENSE)

**The firewall for MCP tool calls.** Block, audit, replay.

Assay sits between your agent and its MCP tools. Every tool call gets an explicit ALLOW or DENY with a replayable evidence trail. No hosted backend, no probabilistic filtering — just deterministic policy enforcement.

## Quick Start

```bash
cargo install assay-cli
```

**1. Set up a demo workspace:**

```bash
mkdir -p /tmp/assay-demo && echo "safe content" > /tmp/assay-demo/safe.txt
```

**2. Wrap an MCP server with policy:**

```bash
assay mcp wrap --policy examples/mcp-quickstart/policy.yaml \
  -- npx @modelcontextprotocol/server-filesystem /tmp/assay-demo
```

**3. See decisions on every tool call:**

```
✅ ALLOW  read_file  path=/tmp/assay-demo/safe.txt  reason=policy_allow
✅ ALLOW  list_dir   path=/tmp/assay-demo/           reason=policy_allow
❌ DENY   read_file  path=/etc/passwd                reason=path_constraint_violation
❌ DENY   exec       cmd=ls                          reason=tool_denied
```

Your MCP server now has a policy gate. See the full [MCP quickstart example](examples/mcp-quickstart/) for details.

## Why Assay

| | |
|---|---|
| **Deterministic** | Same input, same decision, every time. Not probabilistic. |
| **MCP-native** | Built for the Model Context Protocol tool-call path. |
| **Evidence trail** | Every decision produces auditable, replayable bundles. |
| **Offline** | No hosted backend. Policies and traces stay on your machine. |
| **Fast** | Single-digit ms overhead per tool call. |
| **Tested** | [Three security experiments](docs/architecture/SYNTHESIS-TRUST-CHAIN-TRIFECTA-2026q2.md) with zero false positives. |

## What Assay Does

### Guard your MCP server

```bash
assay mcp wrap --policy policy.yaml -- your-mcp-server
```

### Gate your CI

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

### Generate policy from behavior

```bash
assay init --from-trace trace.jsonl
```

### Audit and replay

```bash
assay evidence export --profile profile.yaml --out evidence.tar.gz
assay evidence verify evidence.tar.gz
assay evidence lint --pack eu-ai-act-baseline evidence.tar.gz
```

## Install

```bash
cargo install assay-cli
```

Or use the [GitHub Action](https://github.com/marketplace/actions/assay-ai-agent-security) directly in CI — no local install needed.

Python SDK: `pip install assay-it`

## Policy

A policy file controls what tools are allowed:

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

Or generate one from observed behavior:

```bash
assay init --from-trace trace.jsonl
```

## Learn More

- [MCP Quickstart Example](examples/mcp-quickstart/)
- [CI Guide](docs/guides/github-action.md)
- [Evidence Store](docs/guides/evidence-store-aws-s3.md) (S3, [B2](docs/guides/evidence-store-backblaze-b2.md), [MinIO](docs/guides/evidence-store-minio.md))
- [Architecture](docs/architecture/index.md)
- [Security Experiments](docs/architecture/SYNTHESIS-TRUST-CHAIN-TRIFECTA-2026q2.md)

## Contributing

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](LICENSE)
