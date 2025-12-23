# Verdict Python SDK

**Record deterministic traces from your Python agents for regression gating.**

## Installation

```bash
pip install verdict-sdk
```

Optional: Install with OpenAI support:
```bash
pip install verdict-sdk[openai]
```

## Quick Start (OpenAI)

Record a deterministic trace (including tool calls + tool results):

```python
import os
import openai
from verdict_sdk import TraceWriter, record_chat_completions_with_tools

client = openai.OpenAI(api_key=os.environ["OPENAI_API_KEY"])
writer = TraceWriter("traces/agent.jsonl")

# 1. Define your tool schemas
TOOLS = [
    {
        "type": "function",
        "function": {
            "name": "GetWeather",
            "description": "Get the current weather for a location.",
            "parameters": {
                "type": "object",
                "properties": {"location": {"type": "string"}},
                "required": ["location"],
            },
        },
    }
]

# 2. Implement your tool executors
def GetWeather(args):
    return {"location": args["location"], "temp_c": 22}

# 3. Execute loop and record automatically
result = record_chat_completions_with_tools(
    writer=writer,
    client=client,
    model="gpt-4o",
    messages=[{"role": "user", "content": "What's the weather in Tokyo?"}],
    tools=TOOLS,
    tool_executors={"GetWeather": GetWeather}, # Name must match schema
    episode_id="weather_demo",
    test_id="weather_demo",
)
print(result["content"])
```

## Run Regression Gate

Use the Verdict CLI to validate the recorded trace:

```bash
verdict ci \
  --config verdict.yaml \
  --trace-file traces/agent.jsonl \
  --replay-strict \
  --db :memory:
```

## Features

- **EpisodeRecorder**: Context manager for manual trace recording.
- **TraceWriter**: Deterministic, append-only JSONL generator with sorted keys.
- **Auto-create Directories**: `TraceWriter("traces/agent.jsonl")` will create `traces/` automatically if missing.
- **OpenAI Instrumentor**: Seamless bridge between OpenAI's SDK and Verdict's storage format.
- **Python 3.8+ Compatible**: Supports modern and legacy Python environments.
