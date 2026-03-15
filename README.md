# Assay

[![Crates.io](https://img.shields.io/crates/v/assay-cli.svg)](https://crates.io/crates/assay-cli)
[![CI](https://github.com/Rul1an/assay/actions/workflows/ci.yml/badge.svg)](https://github.com/Rul1an/assay/actions/workflows/ci.yml)
[![License](https://img.shields.io/crates/l/assay-core.svg)](https://github.com/Rul1an/assay/blob/main/LICENSE)

**The firewall for MCP tool calls.** Block unsafe calls, audit every decision, replay anything.

Assay wraps your MCP server with deterministic policy enforcement. Every tool call gets an explicit ALLOW or DENY with an auditable evidence trail. No hosted backend required.

## Quick Start

```bash
cargo install assay-cli
```

Wrap any MCP server and see policy decisions in real-time:

```bash
assay mcp wrap -- npx @modelcontextprotocol/server-filesystem ./
```

Every tool call now shows a clear decision:

```
✅ ALLOW  read_file  path=/app/src/main.rs  reason=policy_allow
✅ ALLOW  list_dir   path=/app/src/         reason=policy_allow
❌ DENY   read_file  path=/etc/passwd       reason=path_constraint_violation
❌ DENY   exec       cmd=rm -rf /           reason=tool_denied
```

That's it. Your MCP server now has a policy gate.

## What You Can Do

**Catch problems locally:**

```bash
# Wrap your server → see decisions instantly
assay mcp wrap -- your-mcp-server

# Generate a policy from observed behavior
assay generate --from-trace trace.jsonl
```

**Gate your CI:**

```yaml
# .github/workflows/assay.yml
- uses: Rul1an/assay-action@v2
```

```bash
# Or run manually
assay ci --config eval.yaml --trace-file traces/golden.jsonl
```

**Audit and replay:**

```bash
assay evidence export --profile profile.yaml --out evidence.tar.gz
assay evidence verify evidence.tar.gz
assay evidence lint --pack eu-ai-act-baseline evidence.tar.gz
assay evidence diff baseline.tar.gz current.tar.gz
```

## Why Assay

- **Deterministic** — same input, same decision, every time. No probabilistic filtering.
- **MCP-native** — built for the Model Context Protocol tool-call path, not retrofitted.
- **Evidence-first** — every decision produces auditable, replayable evidence bundles.
- **Offline** — runs locally with no hosted backend. Your policies and traces stay on your machine.
- **Fast** — single-digit millisecond overhead per tool call in published benchmarks.
- **Open source** — MIT licensed, open core model.

## Install

```bash
cargo install assay-cli
```

Also available via the [GitHub Action](https://github.com/marketplace/actions/assay-ai-agent-security) for CI.

## Configuration

Two files, minimal config:

**`policy.yaml`** — what tools are allowed:

```yaml
version: "1.0"
name: "my-policy"
allow: ["*"]
deny: ["exec", "shell", "bash"]
constraints:
  - tool: "read_file"
    params:
      path:
        matches: "^/app/.*"
```

**`eval.yaml`** — what to test:

```yaml
version: 1
suite: "my_agent"
model: "trace"
tests:
  - id: "safe_deploy"
    input:
      prompt: "deploy staging"
    expected:
      type: args_valid
      schema:
        deploy_service:
          type: object
          required: [env]
          properties:
            env:
              enum: [staging, prod]
```

Or skip config entirely and start from observed behavior:

```bash
assay init --from-trace trace.jsonl
```

## Evidence & Compliance

Assay produces tamper-evident `.tar.gz` bundles with manifests, content-addressed hashes, and event streams.

```bash
assay evidence export --profile profile.yaml --out bundle.tar.gz
assay evidence verify bundle.tar.gz
assay evidence push bundle.tar.gz --store s3://my-bucket/evidence
assay evidence store-status
```

Compliance packs map to regulatory frameworks:

```bash
assay evidence lint --pack eu-ai-act-baseline bundle.tar.gz
assay evidence lint --pack cicd-starter bundle.tar.gz
```

## Security Research

Assay's trust chain has been tested through three bounded security experiments:

| Experiment | Perspective | Result |
|-----------|------------|--------|
| [Memory Poisoning](docs/architecture/RESULTS-EXPERIMENT-MEMORY-POISON-2026q2.md) | Producer | 0% delayed activation under full stack |
| [Delegation Spoofing](docs/architecture/RESULTS-EXPERIMENT-DELEGATION-SPOOFING-2026q2.md) | Adapter | 0/4 bypass under full stack |
| [Protocol Evidence](docs/architecture/RESULTS-EXPERIMENT-PROTOCOL-EVIDENCE-INTERPRETATION-2026q2.md) | Consumer | 100% canonical agreement under full stack |

Zero false positives across all experiments. See the [synthesis](docs/architecture/SYNTHESIS-TRUST-CHAIN-TRIFECTA-2026q2.md) for the full analysis.

## Documentation

- [MCP Quickstart](docs/mcp/quickstart.md)
- [CI Guide](docs/guides/github-action.md)
- [Evidence Store Setup](docs/guides/evidence-store-aws-s3.md) (S3, [B2](docs/guides/evidence-store-backblaze-b2.md), [MinIO](docs/guides/evidence-store-minio.md))
- [Architecture](docs/architecture/index.md)
- [Roadmap](docs/ROADMAP.md)

## Python

```bash
pip install assay-it
```

## Contributing

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](LICENSE)
