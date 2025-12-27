# assay import

Import agent sessions from MCP Inspector and other formats.

---

## Synopsis

```bash
assay import --format <FORMAT> <INPUT_FILE> [OPTIONS]
```

---

## Description

Converts agent session logs into Assay's normalized trace format. This is typically the first step in setting up Assay for a new project.

---

## Options

### Required

| Option | Description |
|--------|-------------|
| `--format`, `-f` | Input format (see supported formats) |
| `<INPUT_FILE>` | Path to the session file |

### Output

| Option | Description |
|--------|-------------|
| `--out-trace`, `-o` | Output trace file path |
| `--out-dir` | Output directory for traces |
| `--init` | Auto-generate config and policy files |

### Processing

| Option | Description |
|--------|-------------|
| `--filter-tools` | Only import specific tools |
| `--exclude-tools` | Exclude specific tools |
| `--start-time` | Filter events after this timestamp |
| `--end-time` | Filter events before this timestamp |

---

## Supported Formats

| Format | Source | Flag |
|--------|--------|------|
| MCP Inspector | [MCP Inspector](https://github.com/modelcontextprotocol/inspector) | `--format mcp-inspector` |
| JSON-RPC 2.0 | Raw MCP messages | `--format jsonrpc` |
| LangChain | LangChain traces | `--format langchain` *(coming soon)* |
| LlamaIndex | LlamaIndex traces | `--format llamaindex` *(coming soon)* |

---

## Examples

### Basic Import

```bash
# From MCP Inspector export
assay import --format mcp-inspector session.json

# Output:
# Imported 47 tool calls from session.json
# Created: traces/session-2025-12-27.jsonl
```

### With Auto-Init

```bash
# Generate config and policies automatically
assay import --format mcp-inspector session.json --init

# Output:
# Imported 47 tool calls from session.json
# Discovered 5 unique tools: get_customer, update_customer, ...
#
# Created:
#   traces/session-2025-12-27.jsonl
#   mcp-eval.yaml
#   policies/default.yaml
#
# Next steps:
#   1. Review policies/default.yaml
#   2. Run: assay run --config mcp-eval.yaml
```

### Custom Output Path

```bash
# Specify output location
assay import --format mcp-inspector session.json \
  --out-trace traces/production-incident.jsonl
```

### Filtering

```bash
# Only import specific tools
assay import --format mcp-inspector session.json \
  --filter-tools get_customer,update_customer

# Exclude tools
assay import --format mcp-inspector session.json \
  --exclude-tools debug_*,internal_*

# Time range
assay import --format mcp-inspector session.json \
  --start-time "2025-12-27T10:00:00Z" \
  --end-time "2025-12-27T11:00:00Z"
```

---

## Input Format: MCP Inspector

MCP Inspector exports sessions as JSON with JSON-RPC 2.0 messages:

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
        "content": [{ "type": "text", "text": "{\"name\": \"Alice\"}" }]
      }
    }
  ]
}
```

To export from MCP Inspector:
1. Run your agent session
2. **File → Export Session → JSON**
3. Save as `session.json`

---

## Output Format: Assay Trace

Assay produces a normalized JSONL trace:

```jsonl
{"type":"tool_call","id":"1","tool":"get_customer","arguments":{"id":"cust_123"},"timestamp":"2025-12-27T10:00:00Z"}
{"type":"tool_result","id":"1","result":{"name":"Alice"},"timestamp":"2025-12-27T10:00:01Z"}
```

---

## Auto-Generated Files

When using `--init`, Assay creates:

### mcp-eval.yaml

```yaml
version: "1"
suite: imported-session

tests:
  - id: args_valid
    metric: args_valid
    policy: policies/default.yaml

  - id: no_blocked_tools
    metric: tool_blocklist
    blocklist: []

output:
  format: [sarif, junit]
  directory: .assay/reports
```

### policies/default.yaml

```yaml
# Auto-generated policy template
# Review and add constraints as needed

tools:
  get_customer:
    arguments:
      id:
        type: string
        # Add: required: true
        # Add: pattern: "^cust_[0-9]+$"
  
  update_customer:
    arguments:
      id:
        type: string
      email:
        type: string
        # Add: format: email
```

---

## Error Handling

### Invalid Format

```
Error: Unknown format 'invalid'

Supported formats:
  - mcp-inspector
  - jsonrpc
  - langchain (coming soon)
  - llamaindex (coming soon)
```

### Parse Error

```
Error: Failed to parse session.json

  Line 15: Expected ',' or '}' but found ':'
  
Suggestion: Validate JSON with 'jq . session.json'
```

### Empty Session

```
Warning: No tool calls found in session.json

The file was parsed successfully but contains no tools/call messages.

Check that:
  1. The session includes tool usage
  2. The export format is correct
```

---

## Batch Import

Import multiple sessions at once:

```bash
# Import all sessions in a directory
for f in sessions/*.json; do
  assay import --format mcp-inspector "$f" --out-dir traces/
done
```

Or use a glob:

```bash
assay import --format mcp-inspector "sessions/*.json" --out-dir traces/
```

---

## See Also

- [Traces](../concepts/traces.md)
- [Import Formats](../mcp/import-formats.md)
- [assay run](run.md)
