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

For sandboxed runs, `assay sandbox --profile out.yaml` also writes a sibling
evidence profile sidecar such as `out.evidence.yaml`. That sidecar is the
machine-readable input for `assay evidence export` when you want bundle evidence
from the sandbox profiling flow, including supported
`assay.sandbox.degraded` events.

### Bundle ID

Each bundle has a content-addressed ID:

```
sha256:a3f2b1c4d5e6f7890...
```

Any modification changes the ID. Tamper-evident by design.

### Comparing Bundles

`assay evidence diff` verifies both bundles, then compares the retained verified events
on two layers:

- **Retained events by content id.** Every event carries a verified `content_hash` over its
  type, subject and payload; `run_root` binds the ordered sequence of those hashes. The report
  prints both `run_root`s and lists events present on one side only, named by content id, type
  and sequence number. Unequal roots are a sound witness that the retained events differ;
  equal roots that they do not.
- **Subject projections.** Added and removed `.net`, `.fs` and `.process` subjects (hosts,
  paths, process names), as before.

The output ends with one of two lines:

```
No differences in retained verified events: run_root equal.
Retained verified events differ: run_root differs; 1 added, 1 removed by content id.
```

Equal projections with a differing `run_root` are reported as differing, never as clean.
Absence and completeness are not established: the comparison covers what both bundles
retained, not what happened. Exit code is `0` whenever both bundles verify and the comparison
completes, with or without differences; a bundle that fails to open or verify is an error.
`--format json` carries the same data under `retained_events` (`run_root_equal`, `added`,
`removed`) next to the existing subject sets.

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
# Check every configured prompt appears verbatim in the trace
assay trace verify --trace traces/golden.jsonl --config eval.yaml

# Output:
# truncation ordinal=1 kind=episode_start episode_id="ep-1" pointer=/input reading=unmeasured
# truncation ordinal=1 kind=episode_start episode_id="ep-1" pointer=/meta reading=unmeasured
# truncation ordinal=2 kind=step episode_id="ep-1" step_id="s1" pointer=/content reading=unmeasured
# ...
# ✅ Trace Verification Passed: All 3 config tests found in trace.
```

Before the coverage verdict, `trace verify` prints one truncation reading per event
occurrence and per field the ingest stage scans (`/input` and `/meta` for an episode start,
`/content` and `/meta` for a step, `/args` and `/result` for a tool call; an episode end
scans nothing and is printed with `fields=none`). The ordinal counts yielded events, so a
V1 record expands to three ordinals; it is a locator, not a JSONL line number or a proof
of origin. Identifiers and stage names are printed as JSON string literals escaped to
printable ASCII.

Readings come from the ADR-050 observation carrier and mean exactly this:

- `lossy`: a loss record covers the field. Truncation happened at some stage and the
  original bytes are not in the trace.
- `measured_clean stage="<name>" ceiling=<bytes>`: the named stage reports it did not
  shorten anything under this field at that ceiling. This is stage-local, not end-to-end
  completeness; an exporter or host upstream may already have cut the value.
- `unmeasured`: no trusted stage reports on the field. Absence of a record is not
  intactness. A value that still carries the in-band `...[TRUNCATED]` mark with no loss
  record also reads `unmeasured`, never clean.

No stage is trusted by default, so a fresh run shows `lossy` or `unmeasured` only. Pass
`--trust-stage <name>` once per stage whose clean report you accept; the ingest stage is
`assay.trace.upgrader`, and trusting it is trusting this local scan, not the original
producer. Trust never turns `lossy` into anything else. The readings are informational: the
exit code is still decided by prompt coverage alone.

When a configured prompt is absent verbatim but its stage-local truncated shape is present
in the trace, the failure report keeps that verdict and cites one line per `EpisodeStart`
occurrence of the retained prompt, each with that occurrence's `/input` reading:

```text
❌ Trace Verification Failed (1 unresolved test):
  • 1 test matches stage-local truncation shape (exact prompt coverage cannot be established):
     - test-0
       ordinal=1 /input reading=lossy
       ordinal=2 /input reading=unmeasured
```

```bash
assay trace verify --trace traces/golden.jsonl --config eval.yaml \
  --trust-stage assay.trace.upgrader
```

If the trace cannot be read to the end, the rows already printed are followed by
`truncation incomplete after_ordinal=<n> error="..."` and the command fails; those rows
describe only the events before the failure.

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
