# Assay Python SDK

**Record deterministic traces from your Python agents for regression gating.**

## 🚀 Golden Quickstart

The fastest way to regression test your AI agent.

### 1. Installation
```bash
pip install assay
```

### 2. Record (`record.py`)
Run your agent through the SDK to capture a trace. Pass your tool functions to `tool_executors` so Assay can record their inputs and outputs.

```python
import os
import openai
from assay_sdk import TraceWriter, record_chat_completions_with_tools

# 1. Setup Client & Tools
client = openai.OpenAI(api_key=os.environ.get("OPENAI_API_KEY", "mock"))
TOOLS = [{
    "type": "function",
    "function": {
        "name": "GetWeather",
        "parameters": {"type": "object", "properties": {"location": {"type": "string"}}}
    }
}]

# 2. Define Execution Logic (The "Real" Code)
def get_weather(args):
    return {"temp": 22, "location": args.get("location")}

# 3. Record the Loop
writer = TraceWriter("traces/quickstart.jsonl")
result = record_chat_completions_with_tools(
    writer=writer,
    client=client,
    model="gpt-4o",
    messages=[{"role": "user", "content": "Weather in Tokyo?"}],
    tools=TOOLS,
    tool_executors={"GetWeather": get_weather}, # Link schema -> function
    episode_id="weather_demo",
    test_id="weather_check"
)
print(f"Agent Final Answer: {result['content']}")
```

### 3. Configure (`assay.yaml`)
Tell Assay what to check.

```yaml
version: 1
model: "trace"
tests:
  - id: weather_check
    input:
      prompt: "Weather in Tokyo?" # Matches the recorded prompt
    expected:
      type: regex_match
      pattern: ".*" # Pass if any content returned (baseline check)
```

### 4. Verify
Run the regression gate. This replays your trace against the recorded tool outputs to ensure determinism.

```bash
# Verify strictly (fails if any tool call arg changed even slightly)
assay ci --config assay.yaml --trace-file traces/quickstart.jsonl --replay-strict --db :memory:
```

---

## 🌊 Advanced: Streaming support
Capture streaming responses while maintaining tool call execution.

```python
from assay_sdk import record_chat_completions_stream_with_tools

# ... setup client & writer ...

result = record_chat_completions_stream_with_tools(
    writer=writer,
    # ... args ...
    stream=True # SDK handles chunk aggregation automatically
    # tool_executors={...} # Required if tools are used
)
```
*Note: The hybrid wrapper (`record_chat_completions_stream_with_tools`) streams the thinking tokens to the user, executes tools, and then performs a standard follow-up call.*

## 🛡️ Advanced: Privacy & Redaction
Protect sensitive data (PII, API keys) from ever hitting the trace file.

```python
from assay_sdk import TraceWriter, make_redactor

# Create a redactor that scrubs keys and regex patterns
redactor = make_redactor(
    key_denylist={"authorization", "password", "api_key"},
    patterns=[r"sk-[a-zA-Z0-9]{20,}"] # Mask OpenAI keys
)

# Attach to writer - happens automatically on write
writer = TraceWriter("traces/secure.jsonl", redact_fn=redactor)
```

## ⚡ Async Support
Native `async` support for high-throughput applications (FastAPI, etc.) is available via the `assay_sdk.async_openai` submodule. It provides full parity with the sync API, including loop and streaming support.

## ❓ Troubleshooting

### `E_TRACE_EPISODE_MISSING`
**Cause**: The `test_id` or `episode_id` in your trace doesn't match what `assay ci` expected from its config (or implicit default).
**Fix**: Ensure your `assay.yaml` test IDs match the `test_id` passed to `record_chat_completions...`.

### "Duplicate prompt in strict replay"
**Cause**: You ran `record.py` twice without cleaning the trace file, so it contains two identical episodes. `assay ci` in strict mode doesn't know which one to replay.
**Fix**:
1. Truncate the file before recording: `trace_path = "traces/my_trace.jsonl"; open(trace_path, 'w').close()`.
2. Use unique `episode_id`s (e.g. UUIDs) for every run.
