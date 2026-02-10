# Assay

[![Crates.io](https://img.shields.io/crates/v/assay-cli.svg)](https://crates.io/crates/assay-cli)
[![CI](https://github.com/Rul1an/assay/actions/workflows/ci.yml/badge.svg)](https://github.com/Rul1an/assay/actions/workflows/ci.yml)
[![License](https://img.shields.io/crates/l/assay-core.svg)](https://github.com/Rul1an/assay/blob/main/LICENSE)

**Policy-as-Code for AI Agents.**

Runs offline. No telemetry. No vendor lock-in.

Assay validates AI agent behavior against policies. Record traces, generate policies, run deterministic CI gates, produce evidence bundles for audit. Works with any MCP-compatible agent.

> **Open Core:** Engine + baseline packs are MIT/Apache-2.0.
> Compliance packs (EU AI Act, SOC2) are commercial.
> See [ADR-016](docs/architecture/ADR-016-Pack-Taxonomy.md).

<div align="center">
  <img src="demo/output/hero.gif" alt="Assay Demo" width="100%" />
</div>

<!-- MP4 version available at demo/output/hero.mp4 for docs sites -->

<div align="center">
  <a href="https://codespaces.new/Rul1an/assay?quickstart=1">
    <img src="https://github.com/codespaces/badge.svg" alt="Open in GitHub Codespaces" width="160" />
  </a>
  &nbsp;&nbsp;&nbsp;
  <code>cargo install assay-cli</code>
</div>

<br/>

## Install

```bash
cargo install assay-cli
```

## Quickstart

### From scratch

```bash
# Generate policy + config from project defaults
assay init --ci

# Run smoke tests (uses bundled traces, no API calls)
assay ci --config ci-eval.yaml --trace-file traces/ci.jsonl
```

### From an existing trace

```bash
# Generate policy from recorded agent behavior
assay init --from-trace trace.jsonl

# Validate
assay validate --config eval.yaml --trace-file trace.jsonl
```

### From an MCP Inspector session

```bash
# Import trace
assay import --format inspector session.json --out-trace traces/session.jsonl

# Run tests
assay run --config eval.yaml --trace-file traces/session.jsonl
```

## Commands

### Testing & Validation

| Command | What it does |
|---------|-------------|
| `assay run` | Execute test suite against trace file and write `run.json`/`summary.json`. |
| `assay ci` | CI-mode run. Adds `--sarif`, `--junit`, `--pr-comment` outputs. |
| `assay validate` | Stateless policy check. Text, JSON, or SARIF output. |
| `assay replay` | Replay from a self-contained bundle (offline, hermetic). |

### Policy & Config

| Command | What it does |
|---------|-------------|
| `assay init` | Scaffold project: policy, config, CI workflow. `--from-trace` for existing traces. |
| `assay generate` | Generate policy from trace or multi-run profile. `--heuristics` for entropy analysis. |
| `assay profile` | Multi-run stability profiling. Wilson interval gating. |
| `assay doctor` | Diagnose config, trace, and baseline issues. |
| `assay explain` | Step-by-step trace explanation against policy. Terminal, markdown, JSON output. |

### Evidence & Compliance

| Command | What it does |
|---------|-------------|
| `assay evidence export` | Create evidence bundle (tar.gz, content-addressed, Merkle root). |
| `assay evidence verify` | Verify bundle integrity. |
| `assay evidence lint` | Lint bundle with SARIF output. Supports `--pack` for compliance rules. |
| `assay evidence diff` | Diff two verified bundles (network, filesystem, process changes). |
| `assay evidence explore` | Interactive TUI explorer. |
| `assay evidence push/pull/list` | BYOS: S3, GCS, Azure Blob, R2, B2, MinIO. |
| `assay bundle create/verify` | Replay bundles (portable, offline test artifacts). |

### Runtime

| Command | What it does |
|---------|-------------|
| `assay mcp wrap` | Wrap an MCP process with policy enforcement (JSON-RPC over stdio). |
| `assay sandbox` | Landlock sandbox execution (Linux, rootless). |
| `assay monitor` | eBPF/LSM runtime enforcement (Linux, requires capabilities). |

### Misc

| Command | What it does |
|---------|-------------|
| `assay sim run` | Attack simulation suite (integrity, chaos, differential). |
| `assay import` | Import traces from MCP Inspector or JSON-RPC logs. |
| `assay tool sign/verify/keygen` | Ed25519 + DSSE tool signing. |
| `assay fix` | Interactive auto-fix suggestions for policy issues. |

## CI Integration

### GitHub Actions

```yaml
- uses: Rul1an/assay/assay-action@v2
```

The action installs assay, runs your gate, uploads SARIF to the Security tab, and posts a PR comment with results.

```yaml
# .github/workflows/assay.yml
name: Assay Gate
on: [push, pull_request]

permissions:
  contents: read
  pull-requests: write
  security-events: write

jobs:
  assay:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: Rul1an/assay/assay-action@v2
```

Or generate a workflow:

```bash
assay init --ci github   # writes .github/workflows/assay.yml
assay init --ci gitlab   # writes .gitlab-ci.yml
```

### Manual CI

```bash
assay ci \
  --config eval.yaml \
  --trace-file traces/golden.jsonl \
  --sarif reports/sarif.json \
  --junit reports/junit.xml \
  --pr-comment reports/pr-comment.md \
  --replay-strict
```

Exit codes: `0` pass, `1` test failure, `2` config error, `3` infra error.

## Configuration

Two files: a test config (`eval.yaml`) and a policy (`policy.yaml`).

**eval.yaml** — defines what to test:
```yaml
version: 1
suite: "my_agent"
model: "trace"
tests:
  - id: "deploy_args"
    input:
      prompt: "deploy_staging"
    expected:
      type: args_valid
      schema:
        deploy_service:
          type: object
          required: [env]
          properties:
            env: { type: string, enum: [staging, prod] }
```

**policy.yaml** — defines what's allowed:
```yaml
version: "1.0"
name: "my-policy"
allow: ["*"]
deny:
  - "exec"
  - "shell"
  - "bash"
constraints:
  - tool: "read_file"
    params:
      path:
        matches: "^/app/.*|^/data/.*"
```

Policy packs: `assay init --pack default|hardened|dev`

## Evidence Bundles

Tamper-evident `.tar.gz` bundles containing `manifest.json` (SHA-256 hashes, Merkle root) and `events.ndjson` (CloudEvents format, content-addressed IDs).

```bash
assay evidence export --profile profile.yaml --out bundle.tar.gz
assay evidence verify bundle.tar.gz
assay evidence lint --pack eu-ai-act-baseline bundle.tar.gz
assay evidence diff baseline.tar.gz current.tar.gz
```

## Python SDK

```bash
pip install assay
```

```python
from assay import AssayClient

client = AssayClient("traces.jsonl")
client.record_trace(tool_call)
```

Pytest plugin:
```python
@pytest.mark.assay(trace_file="test_traces.jsonl")
def test_agent():
    pass
```

## Project Structure

```
crates/
  assay-cli/        CLI binary
  assay-core/       Eval engine, store, trace replay, report formatters
  assay-metrics/    Built-in metrics (args_valid, sequence_valid, regex_match, etc.)
  assay-evidence/   Evidence bundles, lint engine, diff, sanitize
  assay-mcp-server/ MCP proxy for runtime enforcement
  assay-sim/        Attack simulation harness
  assay-monitor/    eBPF/LSM runtime (Linux)
  assay-policy/     Policy compilation (kernel + userspace tiers)
  assay-registry/   Pack registry client (DSSE, OIDC, lockfile)
  assay-common/     Shared types
  assay-ebpf/       Kernel eBPF programs
assay-python-sdk/   Python SDK (PyO3 + pytest plugin)
```

## Contributing

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](LICENSE)
