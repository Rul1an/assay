# Traces

Traces are recorded agent sessions — the "golden" behavior you test against.

---

## What is a Trace?

A **trace** is a normalized log of every tool call your AI agent made during a session:

- Which tools were called
- What arguments were passed
- What results were returned
- In what order

Traces are the foundation of Assay's deterministic testing. Instead of calling your LLM again (slow, expensive, flaky), Assay replays the recorded trace and validates it against your policies.

---

## Trace Format

Assay uses a line-delimited JSON format (`.jsonl`):

```jsonl
{"type":"tool_call","id":"call_001","tool":"get_customer","arguments":{"id":"cust_123"},"timestamp":"2025-12-27T10:00:00Z"}
{"type":"tool_result","id":"call_001","result":{"name":"Alice","email":"alice@example.com"},"timestamp":"2025-12-27T10:00:01Z"}
{"type":"tool_call","id":"call_002","tool":"update_customer","arguments":{"id":"cust_123","email":"alice@newdomain.com"},"timestamp":"2025-12-27T10:00:02Z"}
{"type":"tool_result","id":"call_002","result":{"success":true},"timestamp":"2025-12-27T10:00:03Z"}
```

Each line is a self-contained event:

| Field | Description |
|-------|-------------|
| `type` | `tool_call` or `tool_result` |
| `id` | Links call to result |
| `tool` | Tool name (for calls) |
| `arguments` | Tool arguments (for calls) |
| `result` | Tool response (for results) |
| `timestamp` | When the event occurred |

---

## Creating Traces

### From MCP Inspector

Export your session from [MCP Inspector](https://github.com/modelcontextprotocol/inspector), then import:

```bash
assay import --format mcp-inspector session.json --init
```

This creates:
- `traces/session-YYYY-MM-DD.jsonl` — The normalized trace
- `mcp-eval.yaml` — Test configuration
- `policies/default.yaml` — Policy template

### From Other Formats

```bash
# Raw JSON-RPC messages
assay import --format jsonrpc messages.json

# LangChain traces (coming soon)
assay import --format langchain run.json

# LlamaIndex traces (coming soon)
assay import --format llamaindex trace.json
```

### Manual Creation

For testing, you can create traces manually:

```bash
cat > traces/test.jsonl << 'EOF'
{"type":"tool_call","id":"1","tool":"get_customer","arguments":{"id":"123"}}
{"type":"tool_result","id":"1","result":{"name":"Test User"}}
EOF
```

---

## Trace Storage

Traces are stored in the `.assay/` directory:

```
your-project/
├── .assay/
│   ├── store.db          # SQLite database (cache, metadata)
│   └── traces/           # Trace files
│       ├── session-001.jsonl
│       └── session-002.jsonl
├── traces/               # Your golden traces (commit these)
│   └── golden.jsonl
└── mcp-eval.yaml
```

**Best practice:** Keep "golden" traces in a `traces/` folder at your repo root and commit them to Git. These are your baseline for regression testing.

---

## Trace Fingerprinting

Assay computes a fingerprint (hash) of each trace to detect changes:

```
Trace: traces/golden.jsonl
Fingerprint: sha256:a3f2b1c4d5e6...
```

If the underlying trace changes, the cache invalidates and tests re-run. This ensures you're always testing against the current baseline.

---

## Working with Traces

### Inspect a Trace

```bash
# List all tools in a trace
assay inspect --trace traces/golden.jsonl --tools

# Output:
# Tools found:
#   - get_customer (5 calls)
#   - update_customer (2 calls)
#   - send_email (1 call)
```

### Validate a Trace

```bash
# Check trace format is valid
assay validate --trace traces/golden.jsonl

# Output:
# ✅ Trace valid: 8 events, 4 tool calls
```

### Compare Traces

```bash
# Diff two traces
assay diff --baseline traces/v1.jsonl --candidate traces/v2.jsonl

# Output:
# + Added: delete_customer (1 call)
# - Removed: verify_identity (was 1 call)
# ~ Changed: update_customer arguments differ
```

---

## Trace Best Practices

### 1. Use Descriptive Names

```
traces/
├── golden-customer-flow.jsonl      # ✅ Clear purpose
├── edge-case-empty-cart.jsonl      # ✅ Specific scenario
└── test1.jsonl                     # ❌ Unclear
```

### 2. Version Your Traces

When agent behavior changes intentionally, create new traces:

```bash
# Old baseline
traces/v1-customer-flow.jsonl

# New baseline after feature addition
traces/v2-customer-flow.jsonl
```

### 3. Keep Traces Small

Large traces slow down testing. Record only what's needed:

- **Good:** 10-50 tool calls covering critical paths
- **Avoid:** 1000+ calls from a full day's logs

### 4. Commit Golden Traces

Your "golden" traces should be in version control:

```bash
git add traces/golden.jsonl
git commit -m "Add golden trace for customer workflow"
```

---

## Trace vs. Live Testing

| Aspect | Trace Replay | Live LLM Call |
|--------|--------------|---------------|
| Speed | 3ms | 3+ seconds |
| Cost | $0.00 | $0.01-$1.00 |
| Determinism | 100% | ~80-95% |
| Network | Not required | Required |
| Use case | CI/CD, regression | Exploration, new features |

**Use traces for:** CI gates, regression testing, debugging production issues.

**Use live calls for:** Developing new features, exploring model behavior.

---

## See Also

- [Importing Traces](../mcp/import-formats.md)
- [Replay Engine](replay.md)
- [Cache & Fingerprints](cache.md)
