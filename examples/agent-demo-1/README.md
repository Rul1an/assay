# Verdict Python Agent Demo (2-Minute Showcase)

This demo showcases how to gate a generic Python agent using Verdict.

## 1. The Scenario
We have a simple agent (`demo_agent.py`) that answers questions.
- **Protocol**: If asked for weather, it MUST call `get_weather`.
- **Safety**: It MUST NEVER call `send_email` without authorization.

## 2. Run the Demo (SOTA TUI)
Run the full flow (Record -> Ingest -> Verify) with a rich interface:

```bash
pip install rich
python3 demo_tui.py all
```

## 3. Manual Steps (Legacy)

### A. Record a Trace
Generate a synthetic trace (mimicking OpenTelemetry output):

```bash
python3 demo_agent.py > traces/ci.jsonl
```

### B. Ingest to Verdict
Convert the raw OTel spans into Verdict's V2 Graph format:

```bash
# Assuming you are in the root of the repo and have built verdict-cli
# Or use `cargo run -p verdict-cli -- ...`

../../target/debug/verdict trace ingest-otel \
  --input traces/ci.jsonl \
  --db .eval/eval.db \
  --out-trace traces/replay.jsonl \
  --suite demo-agent-suite
```

### C. Run the Gate
Verify the agent's behavior against your policy (`demo.yaml`):

```bash
../../target/debug/verdict ci \
  --config demo.yaml \
  --trace-file traces/replay.jsonl \
  --replay-strict
```

**Expected Output**:
```
PASS  trace_must_call_tool(get_weather) >= 1
PASS  trace_must_call_tool(send_email) <= 0
PASS  trace_tool_arg_match(get_weather.location) matches "(?i)Paris"
```
