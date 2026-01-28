# Assay

[![Crates.io](https://img.shields.io/crates/v/assay-cli.svg)](https://crates.io/crates/assay-cli)
[![CI](https://github.com/Rul1an/assay/actions/workflows/ci.yml/badge.svg)](https://github.com/Rul1an/assay/actions/workflows/ci.yml)
[![License](https://img.shields.io/crates/l/assay-core.svg)](https://github.com/Rul1an/assay/blob/main/LICENSE)

**Policy-as-Code for AI Agents.**
Deterministic testing, runtime enforcement, and verifiable evidence for the Model Context Protocol.

## Install

```bash
curl -fsSL https://getassay.dev/install.sh | sh
```

Or via Cargo:
```bash
cargo install assay-cli
```

## Core Workflow

### 1. Record → Replay → Validate

Record agent behavior once, replay deterministically in CI. No LLM calls, no flakiness.

```bash
# Capture traces from your agent
assay import --format mcp-inspector session.json --out trace.jsonl

# Validate against policy (milliseconds, $0 cost)
assay validate --config assay.yaml --trace-file trace.jsonl

# CI gate with SARIF output
assay run --config assay.yaml --format sarif
```

### 2. Generate Policies from Behavior

```bash
# Single trace → policy
assay generate -i trace.jsonl --heuristics

# Multi-run profiling for stable policies
assay profile init --output profile.yaml --name my-app
assay profile update --profile profile.yaml -i trace.jsonl --run-id ci-123
assay generate --profile profile.yaml --min-stability 0.8
```

### 3. Evidence Bundles (Audit/Compliance)

Tamper-evident bundles with content-addressed IDs. CloudEvents v1.0 format.

```bash
# Export evidence
assay evidence export --profile profile.yaml --out bundle.tar.gz

# Verify integrity
assay evidence verify bundle.tar.gz

# Lint for security issues (SARIF)
assay evidence lint bundle.tar.gz --format sarif

# Compare runs
assay evidence diff baseline.tar.gz current.tar.gz
```

## Runtime Enforcement

### MCP Server Proxy

```bash
# Start policy enforcement proxy
assay mcp-server --policy policy.yaml
```

### Kernel-Level Sandbox (Linux)

```bash
# Landlock isolation (rootless)
assay sandbox --policy policy.yaml -- python agent.py

# eBPF/LSM enforcement (requires capabilities)
sudo assay monitor --policy policy.yaml --pid <agent-pid>
```

## Configuration

`assay.yaml`:
```yaml
version: "2.0"
name: "mcp-default-gate"

allow: ["*"]

deny:
  - "exec*"
  - "shell*"

constraints:
  - tool: "read_file"
    params:
      path:
        matches: "^/app/.*|^/data/.*"
```

## Python SDK

```bash
pip install assay
```

```python
from assay import AssayClient, validate

# Record traces
client = AssayClient("traces.jsonl")
client.record_trace(tool_call)

# Validate
result = validate("policy.yaml", traces)
assert result["passed"]
```

Pytest plugin for automatic trace capture:
```python
@pytest.mark.assay(trace_file="test_traces.jsonl")
def test_agent():
    pass
```

## Documentation

- [Getting Started](https://getassay.dev/docs/quickstart)
- [Policy Reference](docs/reference/policies.md)
- [Evidence Contract](docs/architecture/ADR-006-Evidence-Contract.md)
- [Runtime Architecture](docs/architecture/runtime.md)
- [Python SDK](docs/python-sdk/)

## Contributing

```bash
cargo test --workspace
```

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](LICENSE)
