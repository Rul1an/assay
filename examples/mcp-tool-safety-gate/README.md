# MCP Tool Safety Gate Example

This example demonstrates how to use Assay to enforce safety policies on Model Context Protocol (MCP) traces.

## Overview
- **Protocol**: MCP (Inspector or JSON-RPC)
- **Goal**: Verify that an agent does NOT call a specific tool (`unsafe_tool`) given a certain prompt.
- **Assay Features**:
    - `assay trace import-mcp`: Converts MCP logs to V2 traces.
    - `assay ci --replay-strict`: Replays the trace deterministically and checks `trace_must_not_call_tool`.

## Prerequisites
- Assay CLI installed (`cargo install --path .`)
- An MCP transcript file (provided: `mcp/session.json`)

## Usage

### 1. Import Trace
Convert the raw MCP log into a Assay trace. We explicitly set the `test-id` to link it to our test case.

```bash
assay trace import-mcp \
  --input mcp/session.json \
  --format inspector \
  --episode-id mcp_demo \
  --test-id mcp_demo \
  --prompt "demo_user_prompt" \
  --out-trace traces/trace.v2.jsonl
```

### 2. Run Gate (Strict Replay)
Run the regression test. We use an in-memory database (`:memory:`) to ensure a clean state for every run. Assay automatically ingests the trace file into this ephemeral DB.

```bash
assay ci \
  --config assay.yaml \
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

      # 1. Setup Assay (or download binary)
      - name: Build Assay
        run: cargo build --bin assay --release

      # 2. Import Transcript (or fetch from artifact store)
      - name: Import MCP Transcript
        run: |
          ./target/release/assay trace import-mcp \
            --input examples/mcp-tool-safety-gate/mcp/session.json \
            --format inspector \
            --episode-id mcp_demo \
            --test-id mcp_demo \
            --prompt "demo_user_prompt" \
            --out-trace trace.jsonl

      # 3. Gate
      - name: Run Assay Gate
        run: |
          ./target/release/assay ci \
            --config examples/mcp-tool-safety-gate/assay.yaml \
            --trace-file trace.jsonl \
            --replay-strict \
            --db :memory:
```
