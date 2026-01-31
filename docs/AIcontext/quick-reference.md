# Quick Reference

> **Purpose**: Fast lookup for AI agents - commands, patterns, exit codes, and common operations.
> **Version**: 2.12.0 (January 2026)

## TL;DR - What is Assay?

**Assay** = Policy-as-Code engine for AI agent validation
- **Input**: Agent traces (JSONL) + Policy (YAML)
- **Output**: Pass/Fail + SARIF report
- **Key insight**: Deterministic replay testing (no LLM calls needed in CI)

## Most Common Commands

```bash
# First-time setup
assay init                    # Generate assay.yaml + policy.yaml
assay init --ci               # Also generate GitHub workflow

# Validate traces
assay validate --trace-file traces.jsonl
assay run --config assay.yaml --trace-file traces.jsonl

# CI gate (strict mode)
assay ci --config assay.yaml --trace-file traces.jsonl

# Debug failures
assay doctor                  # Diagnose common issues
assay explain --trace-file traces.jsonl  # Explain violations
```

## Exit Codes

| Code | Name | Reason Code Pattern | When |
|------|------|---------------------|------|
| 0 | SUCCESS | (none) | All tests pass |
| 1 | TEST_FAILURE | `E_TEST_FAILED`, `E_POLICY_VIOLATION` | Test or policy failure |
| 2 | CONFIG_ERROR | `E_CFG_PARSE`, `E_TRACE_NOT_FOUND`, `E_MISSING_CONFIG` | Config or input error |
| 3 | INFRA_ERROR | `E_JUDGE_UNAVAILABLE`, `E_RATE_LIMIT`, `E_TIMEOUT` | Infrastructure issue |

**Migration note**: Use `--exit-codes=v2` (default) or `--exit-codes=v1` for legacy behavior.

## Reason Code Registry

### Config Errors (exit 2)
| Code | Meaning | Next Step |
|------|---------|-----------|
| `E_CFG_PARSE` | YAML/JSON parse error | `assay doctor --config <file>` |
| `E_TRACE_NOT_FOUND` | Trace file missing | Check path exists |
| `E_MISSING_CONFIG` | Config file missing | `assay init` |
| `E_BASELINE_INVALID` | Baseline file invalid | `assay baseline record` |
| `E_POLICY_PARSE` | Policy syntax error | `assay policy validate <file>` |

### Infra Errors (exit 3)
| Code | Meaning | Next Step |
|------|---------|-----------|
| `E_JUDGE_UNAVAILABLE` | LLM judge down | Check API key, retry |
| `E_RATE_LIMIT` | Rate limited | Wait, reduce concurrency |
| `E_PROVIDER_5XX` | Provider error | Retry, check status page |
| `E_TIMEOUT` | Request timeout | Increase timeout, check network |

### Test Failures (exit 1)
| Code | Meaning | Next Step |
|------|---------|-----------|
| `E_TEST_FAILED` | Test assertion failed | `assay explain <test-id>` |
| `E_POLICY_VIOLATION` | Policy rule violated | Review policy or fix agent |
| `E_SEQUENCE_VIOLATION` | Wrong tool call order | Check sequence rules |

## File Locations

| File | Purpose | Created By |
|------|---------|------------|
| `assay.yaml` | Main config | `assay init` |
| `policy.yaml` | Policy rules | `assay init` |
| `traces/*.jsonl` | Agent traces | SDK or import |
| `baseline.json` | Regression baseline | `assay run --export-baseline` |
| `.github/workflows/assay.yml` | CI workflow | `assay init --ci` |
| `.assay/reports/junit.xml` | JUnit output | `assay run --junit` |
| `.assay/reports/sarif.json` | SARIF output | `assay run --sarif` |
| `.assay/evidence/*.tar.gz` | Evidence bundles | Test runs |

## GitHub Action Usage

```yaml
# Recommended (v2 action)
- uses: Rul1an/assay/assay-action@v2
  with:
    fail_on: error      # error | warn | info | none
    sarif: true         # Upload to Security tab
    comment_diff: true  # PR comment on findings

# Alternative (CLI only)
- run: |
    assay ci \
      --config assay.yaml \
      --trace traces/ci.jsonl \
      --output-dir .assay-reports \
      --junit .assay-reports/junit.xml \
      --sarif .assay-reports/sarif.json
```

## Policy Quick Reference

```yaml
# policy.yaml structure
version: "1"

tools:
  filesystem_read:
    args:
      path:
        type: string
        pattern: "^/allowed/.*"

  http_request:
    args:
      url:
        blocklist:
          - "*.internal.*"

sequences:
  - name: auth_before_data
    pattern: [authenticate, fetch_data]
    required: true

blocklist:
  - "rm_rf"
  - "drop_database"
```

## Trace Format

```jsonl
{"tool": "filesystem_read", "args": {"path": "/tmp/file.txt"}, "result": "contents..."}
{"tool": "http_request", "args": {"url": "https://api.example.com"}, "result": {"status": 200}}
```

## Python SDK Quick Start

```python
from assay import AssayClient, Coverage, validate

# Record traces
client = AssayClient("traces.jsonl")
client.record_trace({"tool": "read_file", "args": {"path": "/tmp/x"}})

# Validate
result = validate("policy.yaml", traces)
assert result["passed"]

# Coverage analysis
coverage = Coverage.analyze(traces, min_coverage=80.0)
print(f"Coverage: {coverage.score}%")
```

## MCP Server Quick Start

```bash
# Start MCP proxy with policy enforcement
assay mcp wrap \
  --policy policy.yaml \
  --decision-log decisions.jsonl \
  --event-source "assay://myapp"

# Dry-run mode (log but don't block)
assay mcp wrap --policy policy.yaml --dry-run
```

## Evidence Commands

```bash
# Export bundle from profile
assay evidence export --profile profile.yaml --out bundle.tar.gz

# Verify bundle integrity
assay evidence verify bundle.tar.gz

# Lint for security issues (SARIF output)
assay evidence lint bundle.tar.gz --format sarif

# Compare two bundles
assay evidence diff baseline.tar.gz current.tar.gz
```

## Tool Signing

```bash
# Generate keypair
assay tool keygen --out keys/

# Sign tool definition
assay tool sign tool.yaml --key keys/private.pem --out tool-signed.yaml

# Verify signature
assay tool verify tool-signed.yaml --trust-policy trust.yaml
```

## Common Patterns

### Pattern 1: CI Gate
```bash
assay run --config assay.yaml --trace-file traces.jsonl --baseline baseline.json
# Exit 0 = merge allowed
# Exit 1 = block PR
```

### Pattern 2: Learning Mode
```bash
assay record --capture --output profile.json
assay generate --from-profile profile.json --output policy.yaml
```

### Pattern 3: Debug Violation
```bash
assay doctor                           # Check setup
assay explain --trace-file traces.jsonl  # Explain failure
assay coverage --trace-file traces.jsonl # Check coverage
```

### Pattern 4: Baseline Regression
```bash
# On main branch
assay run --config assay.yaml --export-baseline baseline.json

# On feature branch
assay run --config assay.yaml --baseline baseline.json
```

## Crate Responsibilities

| Crate | Responsibility | Key Types |
|-------|----------------|-----------|
| `assay-core` | Evaluation engine | `Runner`, `Store`, `EvalConfig` |
| `assay-cli` | CLI interface | `Cli`, `Command`, dispatchers |
| `assay-metrics` | Metric implementations | `MustContain`, `JsonSchema`, etc. |
| `assay-mcp-server` | MCP proxy | `McpProxy`, JSON-RPC handlers |
| `assay-policy` | Policy compilation | `CompiledPolicy`, Tier 1/2 |
| `assay-evidence` | Evidence bundles | `BundleWriter`, `Manifest` |
| `assay-monitor` | eBPF monitoring | Linux kernel integration |

## Key Paths in Codebase

```
crates/assay-cli/src/cli/commands/mod.rs  # Command dispatch
crates/assay-core/src/engine/runner.rs    # Test execution
crates/assay-core/src/storage/store.rs    # SQLite persistence
crates/assay-core/src/mcp/proxy.rs        # MCP proxy
crates/assay-core/src/report/sarif.rs     # SARIF output
crates/assay-cli/src/templates.rs         # CI templates
infra/bpf-runner/health_check.sh          # Runner health
.github/workflows/kernel-matrix.yml       # eBPF CI
```

## Environment Variables

| Variable | Purpose | Default |
|----------|---------|---------|
| `RUST_LOG` | Log level | `info` |
| `ASSAY_EXIT_CODES` | Exit code version | `v2` |
| `OPENAI_API_KEY` | LLM API key | (required for judge) |

## Related Documentation

- [Decision Trees](decision-trees.md) - When to use which approach
- [Entry Points](entry-points.md) - Full command reference
- [Codebase Overview](codebase-overview.md) - Architecture details
