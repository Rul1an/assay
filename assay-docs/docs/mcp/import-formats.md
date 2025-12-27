# Import Formats

Supported log formats for importing traces into Assay.

---

## Overview

Assay can import agent sessions from various sources:

| Format | Source | Status |
|--------|--------|--------|
| `mcp-inspector` | [MCP Inspector](https://github.com/modelcontextprotocol/inspector) | ✅ Supported |
| `jsonrpc` | Raw JSON-RPC 2.0 messages | ✅ Supported |
| `langchain` | LangChain traces | 🔜 Coming soon |
| `llamaindex` | LlamaIndex traces | 🔜 Coming soon |

---

## MCP Inspector

The primary format for MCP-based agents.

### Export from MCP Inspector

1. Run your agent session in MCP Inspector
2. **File → Export Session → JSON**
3. Save as `session.json`

### Import

```bash
assay import --format mcp-inspector session.json
```

### Format Structure

```json
{
  "messages": [
    {
      "jsonrpc": "2.0",
      "id": 1,
      "method": "tools/call",
      "params": {
        "name": "get_customer",
        "arguments": { "id": "cust_123" }
      }
    },
    {
      "jsonrpc": "2.0",
      "id": 1,
      "result": {
        "content": [
          { "type": "text", "text": "{\"name\": \"Alice\"}" }
        ],
        "isError": false
      }
    }
  ]
}
```

### Field Mapping

| MCP Inspector | Assay Trace |
|---------------|-------------|
| `params.name` | `tool` |
| `params.arguments` | `arguments` |
| `result.content` | `result` |
| `id` | Links call to result |

---

## JSON-RPC 2.0

Raw JSON-RPC messages, useful for custom MCP implementations.

### Import

```bash
assay import --format jsonrpc messages.json
```

### Format Structure

```json
[
  {
    "jsonrpc": "2.0",
    "id": "call_001",
    "method": "tools/call",
    "params": {
      "name": "apply_discount",
      "arguments": { "percent": 25 }
    }
  },
  {
    "jsonrpc": "2.0",
    "id": "call_001",
    "result": { "success": true }
  }
]
```

### Notes

- Array of messages (not wrapped in `messages` object)
- `id` field links requests to responses
- Only `tools/call` method is processed

---

## LangChain (Coming Soon)

Import from LangChain's tracing format.

### Expected Usage

```bash
assay import --format langchain langchain_trace.json
```

### Format (Preview)

```json
{
  "runs": [
    {
      "id": "run_abc123",
      "name": "Tool",
      "inputs": {
        "tool": "get_customer",
        "tool_input": { "id": "cust_123" }
      },
      "outputs": {
        "output": { "name": "Alice" }
      }
    }
  ]
}
```

### Status

Currently in development. Track progress at [GitHub Issue #42](https://github.com/Rul1an/assay/issues/42).

---

## LlamaIndex (Coming Soon)

Import from LlamaIndex's instrumentation.

### Expected Usage

```bash
assay import --format llamaindex llamaindex_trace.json
```

### Status

Planned for v1.1. Track progress at [GitHub Issue #43](https://github.com/Rul1an/assay/issues/43).

---

## Output Format

All imports produce Assay's normalized trace format:

```jsonl
{"type":"tool_call","id":"1","tool":"get_customer","arguments":{"id":"cust_123"},"timestamp":"2025-12-27T10:00:00Z"}
{"type":"tool_result","id":"1","result":{"name":"Alice"},"timestamp":"2025-12-27T10:00:01Z"}
```

### Fields

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | `tool_call` or `tool_result` |
| `id` | string | Links call to result |
| `tool` | string | Tool name (calls only) |
| `arguments` | object | Tool arguments (calls only) |
| `result` | any | Tool response (results only) |
| `timestamp` | string | ISO 8601 timestamp |

---

## Custom Formats

For unsupported formats, convert to JSON-RPC manually:

```python
import json

def convert_custom_to_jsonrpc(custom_log):
    messages = []
    for i, entry in enumerate(custom_log):
        # Request
        messages.append({
            "jsonrpc": "2.0",
            "id": str(i),
            "method": "tools/call",
            "params": {
                "name": entry["tool_name"],
                "arguments": entry["inputs"]
            }
        })
        # Response
        messages.append({
            "jsonrpc": "2.0",
            "id": str(i),
            "result": entry["outputs"]
        })
    return messages

# Save and import
with open("converted.json", "w") as f:
    json.dump(convert_custom_to_jsonrpc(my_log), f)
```

Then import:

```bash
assay import --format jsonrpc converted.json
```

---

## Troubleshooting

### "No tool calls found"

The session might not contain `tools/call` messages:

```bash
# Check what methods are in the file
cat session.json | jq '.messages[].method' | sort | uniq
```

### "Invalid JSON"

Validate the file:

```bash
jq . session.json > /dev/null
# If no output, JSON is valid
# If error, fix the syntax
```

### "Missing required field"

Check that each call has:
- `params.name` (tool name)
- `params.arguments` (can be empty `{}`)

---

## See Also

- [assay import](../cli/import.md)
- [Traces](../concepts/traces.md)
- [MCP Quick Start](quickstart.md)
