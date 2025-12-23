# MCP Tool Safety Gate Example

This example demonstrates how to use Verdict to enforce safety policies on Model Context Protocol (MCP) traces.

## Overview
- **Protocol**: MCP (Inspector or JSON-RPC)
- **Goal**: Verify that an agent does NOT call a specific tool (`unsafe_tool`) given a certain prompt.
- **Verdict Features**:
    - `verdict trace import-mcp`: Converts MCP logs to V2 traces.
    - `verdict ci --replay-strict`: Replays the trace deterministically and checks `trace_must_not_call_tool`.

## Prerequisites
- Verdict CLI installed (`cargo install --path .`)
- An MCP transcript file (provided: `mcp/session.json`)

## Usage

### 1. Import Trace
Convert the raw MCP log into a Verdict trace. We explicitly set the `test-id` to link it to our test case.

```bash
verdict trace import-mcp \
  --input mcp/session.json \
  --format inspector \
  --episode-id mcp_demo \
  --test-id mcp_demo \
  --prompt "demo_user_prompt" \
  --out-trace traces/trace.v2.jsonl
```

### 2. Run Gate (Strict Replay)
Run the regression test. We use an in-memory database (`:memory:`) to ensure a clean state for every run. Verdict automatically ingests the trace file into this ephemeral DB.

```bash
verdict ci \
  --config verdict.yaml \
  --trace-file traces/trace.v2.jsonl \
  --replay-strict \
  --db :memory:
```

Output:
```
auto-ingest: loaded 6 events into :memory: (from traces/trace.v2.jsonl)
Running 1 tests...
✅ mcp_demo             1.00  (0.0s)
```

## CI/CD Integration (GitHub Actions)

Copy this workflow to `.github/workflows/mcp-gate.yml` to run this check on every PR.

```yaml
name: MCP Gate
on: [pull_request]

jobs:
  gate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      # 1. Setup Verdict (or download binary)
      - name: Build Verdict
        run: cargo build --bin verdict --release

      # 2. Import Transcript (or fetch from artifact store)
      - name: Import MCP Transcript
        run: |
          ./target/release/verdict trace import-mcp \
            --input examples/mcp-tool-safety-gate/mcp/session.json \
            --format inspector \
            --episode-id mcp_demo \
            --test-id mcp_demo \
            --prompt "demo_user_prompt" \
            --out-trace trace.jsonl

      # 3. Gate
      - name: Run Verdict Gate
        run: |
          ./target/release/verdict ci \
            --config examples/mcp-tool-safety-gate/verdict.yaml \
            --trace-file trace.jsonl \
            --replay-strict \
            --db :memory:
```
