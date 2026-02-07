# assay import

Import an MCP transcript and convert it to Assay trace JSONL.

---

## Synopsis

```bash
assay import <INPUT_FILE> [OPTIONS]
```

---

## Options

| Option | Description |
|--------|-------------|
| `--format <inspector\|jsonrpc>` | Input format (default: `inspector`) |
| `--out-trace <PATH>` | Output trace path (default: `<input>.trace.jsonl`) |
| `--init` | Generate starter scaffolding after import |

Accepted alias for `--format inspector`: `mcp-inspector`.

---

## Examples

```bash
# Basic import from MCP Inspector
assay import session.json --format inspector

# Explicit output path
assay import session.json --format inspector --out-trace traces/session.jsonl

# Import + scaffolding
assay import session.json --format inspector --init
```

---

## Output

The command writes normalized Assay V2 trace events to JSONL.

When `--init` is used, the current implementation generates legacy MCP scaffolding (`mcp-eval.yaml`) in addition to the trace file.

---

## Troubleshooting

- `unknown format`: use `inspector` or `jsonrpc`.
- Parse errors: validate your transcript JSON first.
- Empty import: confirm the transcript actually contains MCP tool traffic.

---

## See Also

- [MCP Import Formats](../../mcp/import-formats.md)
- [assay run](run.md)
- [Traces Concept](../../concepts/traces.md)
