<h1 align="center">
  <br>
  Assay
  <br>
</h1>

<h4 align="center">The purity test for AI — Zero-flake CI for AI agents</h4>

<p align="center">
  <a href="https://github.com/Rul1an/assay/actions">
    <img src="https://github.com/Rul1an/assay/workflows/CI/badge.svg" alt="CI">
  </a>
  <a href="https://crates.io/crates/assay">
    <img src="https://img.shields.io/crates/v/assay.svg" alt="Crates.io">
  </a>
  <a href="https://pypi.org/project/assay/">
    <img src="https://img.shields.io/pypi/v/assay.svg" alt="PyPI">
  </a>
  <a href="https://docs.assay.dev">
    <img src="https://img.shields.io/badge/docs-assay.dev-blue" alt="Docs">
  </a>
  <a href="LICENSE">
    <img src="https://img.shields.io/badge/license-MIT-green" alt="License">
  </a>
</p>

<p align="center">
  <a href="#quick-start">Quick Start</a> •
  <a href="#why-assay">Why Assay</a> •
  <a href="#how-it-works">How It Works</a> •
  <a href="#documentation">Docs</a> •
  <a href="#contributing">Contributing</a>
</p>

---

## The Problem

Your AI agent tests are **flaky**. OpenAI times out, the model hallucinates slightly differently, and your CI fails randomly. Developers stop trusting the pipeline.

Traditional test: `PR → Call LLM → Wait 3 min → Random failure → Retry → 😤`

## The Solution

**Assay** records your agent's behavior once, then replays it locally — **no API calls, no network, no flakiness**.

Assay test: `PR → Replay trace → 3ms → Deterministic pass/fail → ✅`

```bash
# Record once
assay import --format mcp-inspector session.json --init

# Test forever (0ms latency, $0 cost)
assay run --config mcp-eval.yaml --strict
```

---

## Quick Start

### Install

```bash
# Python
pip install assay

# Rust
cargo install assay
```

### Run Your First Test

```bash
# 1. Import an MCP session (creates config automatically)
assay import --format mcp-inspector session.json --init

# 2. Run tests
assay run --config mcp-eval.yaml
```

Output:
```
Assay v0.8.0 — Zero-Flake CI for AI Agents

┌───────────────────┬────────┬─────────────────────────┐
│ Test              │ Status │ Details                 │
├───────────────────┼────────┼─────────────────────────┤
│ args_valid        │ ✅ PASS │ 2ms                     │
│ sequence_valid    │ ✅ PASS │ 1ms                     │
│ tool_blocklist    │ ❌ FAIL │ admin_delete called!    │
└───────────────────┴────────┴─────────────────────────┘

Total: 3ms | 2 passed, 1 failed
```

### Add to CI

```yaml
# .github/workflows/agent-tests.yml
- uses: Rul1an/assay-action@v1
  with:
    config: mcp-eval.yaml
```

---

## Why Assay

|  | Traditional LLM Tests | Assay |
|--|----------------------|-------|
| **Speed** | 30s - 3min | **3ms** |
| **Cost** | $0.10 - $1.00/run | **$0.00** |
| **Flakiness** | 5-20% | **0%** |
| **Network** | Required | **None** |
| **Privacy** | Data sent to APIs | **Local only** |

### Use Cases

- **🔒 Air-Gapped Enterprise** — Banks, healthcare, defense. No data leaves your perimeter.
- **⚡ High-Frequency CI** — 20 PRs/day × 50 tests = $500/day with GPT-4. $0 with Assay.
- **🎯 MCP Native** — Deep integration with Model Context Protocol. Test tool calls, not text.
- **🤖 Agent Self-Correction** — Agents validate their own actions at runtime.

---

## How It Works

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│  1. Record      │ ──► │  2. Define      │ ──► │  3. Replay      │
│  Agent Session  │     │  Policies       │     │  & Validate     │
└─────────────────┘     └─────────────────┘     └─────────────────┘
        │                       │                       │
        ▼                       ▼                       ▼
   session.json            mcp-eval.yaml           Pass/Fail
   (MCP Inspector)         (Your rules)            (Instant)
```

**Policies** define what "correct" means:

```yaml
# mcp-eval.yaml
tests:
  - id: discount_args
    metric: args_valid
    tool: apply_discount
    constraints:
      percent: { max: 30 }
  
  - id: read_before_write
    metric: sequence_valid
    rules:
      - type: before
        first: GetCustomer
        then: UpdateCustomer
```

---

## MCP Integration

Assay is built for the [Model Context Protocol](https://modelcontextprotocol.io).

### Import MCP Sessions

```bash
# From MCP Inspector
assay import --format mcp-inspector session.json

# Auto-generate policies from a "golden" session
assay import --format mcp-inspector good_run.json --init
```

### Assay as MCP Server

Let agents validate their own actions at runtime:

```bash
# Start Assay as an MCP tool server
assay mcp-server --port 3001 --policy policies/
```

The agent can call `assay_check_args` before executing:

```json
{
  "tool": "assay_check_args",
  "arguments": {
    "target_tool": "apply_discount",
    "args": { "percent": 50 }
  }
}
```

Response:
```json
{
  "allowed": false,
  "violations": [{"field": "percent", "constraint": "max: 30"}],
  "suggested_fix": {"percent": 30}
}
```

---

## Documentation

📚 **Full documentation:** [docs.assay.dev](https://docs.assay.dev)

- [Getting Started](https://docs.assay.dev/getting-started/)
- [Configuration Guide](https://docs.assay.dev/config/)
- [MCP Integration](https://docs.assay.dev/mcp/)
- [CLI Reference](https://docs.assay.dev/cli/)
- [Metrics Reference](https://docs.assay.dev/metrics/)

---

## Architecture

Assay is a Rust workspace with four crates:

| Crate | Role |
|-------|------|
| `assay-core` | Trace ingestion, replay engine, cache, storage |
| `assay-cli` | CLI interface, commands, formatting |
| `assay-metrics` | Pure validation functions (`args_valid`, etc.) |
| `assay-mcp-server` | MCP server for agent self-correction |

```
assay/
├── crates/
│   ├── assay-core/
│   ├── assay-cli/
│   ├── assay-metrics/
│   └── assay-mcp-server/
├── docs/
└── examples/
```

---

## Contributing

We welcome contributions! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

```bash
# Clone
git clone https://github.com/Rul1an/assay.git
cd assay

# Build
cargo build

# Test
cargo test

# Run locally
cargo run -- run --config examples/mcp-eval.yaml
```

---

## License

MIT License. See [LICENSE](LICENSE).

---

## Why "Assay"?

> *In metallurgy, an **assay** determines the purity of precious metals.*
> 
> *In software, Assay determines the quality of your AI.*

---

<p align="center">
  <sub>Built with ❤️ for developers who are tired of flaky AI tests.</sub>
</p>
