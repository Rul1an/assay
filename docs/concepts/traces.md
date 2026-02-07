# Traces and Evidence

Traces are recorded agent sessions. Evidence bundles are verifiable, tamper-evident packages of those traces for audit and compliance.

---

## Traces

A **trace** is a normalized log of every tool call your agent made:

- Which tools were called
- What arguments were passed
- What results were returned
- In what order

Traces enable deterministic testing. Replay recorded behavior instead of calling your LLM again.

---

## Evidence Bundles

An **evidence bundle** is a tamper-evident package containing:

- Trace data (CloudEvents v1.0 format)
- Metadata (run ID, timestamps, tool manifest)
- Content-addressed ID (SHA-256)
- Optional signatures (Ed25519, mandate signatures)

```bash
# Create bundle
assay evidence export --profile assay-profile.yaml --out bundle.tar.gz

# Verify integrity
assay evidence verify bundle.tar.gz

# Lint for issues
assay evidence lint bundle.tar.gz --format sarif

# Lint with compliance pack
assay evidence lint --pack eu-ai-act-baseline bundle.tar.gz

# Compare bundles
assay evidence diff baseline.tar.gz current.tar.gz
```

### Bundle ID

Each bundle has a content-addressed ID:

```
sha256:a3f2b1c4d5e6f7890...
```

Any modification changes the ID. Tamper-evident by design.

### BYOS Storage

Push bundles to your own S3-compatible storage:

```bash
assay evidence push bundle.tar.gz --store s3://bucket/evidence
assay evidence pull --bundle-id sha256:abc... --store s3://bucket/evidence
assay evidence list --store s3://bucket/evidence
```

Supported: AWS S3, Backblaze B2, Cloudflare R2, MinIO, Azure Blob, GCS.

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
assay import --format inspector session.json --out-trace traces/session.jsonl
```

This creates:
- `traces/session.jsonl` — The normalized trace

If you use `--init`, the current implementation still scaffolds legacy `mcp-eval.yaml`.

### From Other Formats

```bash
# Raw JSON-RPC messages
assay import --format jsonrpc messages.json
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
└── eval.yaml
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
awk -F'"' '/"tool"/ {print $4}' traces/golden.jsonl | sort | uniq -c

# Output:
#   5 get_customer
#   2 update_customer
#   1 send_email
```

### Validate a Trace

```bash
# Check trace format is valid
assay trace verify --trace traces/golden.jsonl --config eval.yaml

# Output:
# ✅ Trace verifies against config coverage
```

### Compare Traces

```bash
# Diff two traces
diff -u traces/v1.jsonl traces/v2.jsonl

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
