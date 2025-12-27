# Cache & Fingerprints

Assay uses intelligent caching to skip redundant work and fingerprinting to detect changes.

---

## Overview

Assay caches test results to avoid re-running unchanged tests:

```
First run:  Trace → Validate → Cache result
Second run: Trace unchanged? → Return cached result (instant)
```

This makes repeated runs nearly instantaneous while ensuring changes are always detected.

---

## How Caching Works

### Cache Keys

Each cached result is keyed by:

1. **Trace fingerprint** — Hash of the trace content
2. **Policy fingerprint** — Hash of the policy files
3. **Config fingerprint** — Hash of mcp-eval.yaml
4. **Assay version** — CLI version string

If any of these change, the cache is invalidated and tests re-run.

### Cache Location

```
.assay/
├── store.db          # SQLite database with cache
├── cache/
│   ├── results/      # Cached test results
│   └── fingerprints/ # Computed hashes
└── traces/
```

---

## Fingerprinting

### Trace Fingerprints

Assay computes a SHA-256 hash of each trace:

```bash
assay fingerprint --trace traces/golden.jsonl

# Output:
# Trace: traces/golden.jsonl
# Fingerprint: sha256:a3f2b1c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0
# Tool calls: 47
# Size: 12.4 KB
```

If the trace content changes (even one character), the fingerprint changes, and cached results are invalidated.

### Policy Fingerprints

Policies are fingerprinted the same way:

```bash
assay fingerprint --policy policies/customer.yaml

# Output:
# Policy: policies/customer.yaml
# Fingerprint: sha256:1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0
# Tools defined: 5
```

---

## Cache Behavior

### Cache Hit (Fast Path)

When nothing has changed:

```bash
$ assay run --config mcp-eval.yaml

# First run:
# Loading trace... done (2ms)
# Running tests... done (15ms)
# Total: 17ms

# Second run:
# Cache hit: all tests unchanged
# Total: 1ms
```

### Cache Miss (Revalidation)

When something changed:

```bash
$ assay run --config mcp-eval.yaml

# After editing policy:
# Cache miss: policy fingerprint changed
# Loading trace... done (2ms)
# Running tests... done (15ms)
# Total: 17ms
```

---

## Cache Commands

### View Cache Status

```bash
assay cache status

# Output:
# Cache location: .assay/store.db
# Cached results: 12
# Cache size: 45 KB
# Oldest entry: 2025-12-20
# Newest entry: 2025-12-27
```

### Clear Cache

```bash
# Clear all cached results
assay cache clear

# Clear specific trace
assay cache clear --trace traces/golden.jsonl
```

### Disable Cache

For debugging or CI:

```bash
# Skip cache entirely
assay run --config mcp-eval.yaml --no-cache

# Use in-memory database (no persistence)
assay run --config mcp-eval.yaml --db :memory:
```

---

## Cache in CI

### Recommended: In-Memory

For CI, use in-memory mode to avoid cache persistence issues:

```yaml
# .github/workflows/tests.yml
- run: assay run --config mcp-eval.yaml --db :memory:
```

### Optional: Persistent Cache

For faster CI runs, cache the `.assay/` directory:

```yaml
# .github/workflows/tests.yml
- uses: actions/cache@v3
  with:
    path: .assay/
    key: assay-${{ hashFiles('traces/**', 'policies/**') }}
    restore-keys: assay-

- run: assay run --config mcp-eval.yaml
```

---

## Fingerprint Validation

Assay validates fingerprints to detect tampering or corruption:

```bash
assay validate --cache

# Output:
# Validating cache integrity...
# ✅ 12/12 entries valid
# Cache is healthy
```

If corruption is detected:

```bash
# Output:
# ❌ 2 entries corrupted
# Corrupted: traces/old.jsonl (fingerprint mismatch)
# Corrupted: policies/legacy.yaml (file missing)
#
# Run 'assay cache clear' to reset
```

---

## Cache Invalidation Rules

The cache automatically invalidates when:

| Change | Invalidates |
|--------|-------------|
| Trace content modified | That trace's results |
| Policy content modified | All results using that policy |
| mcp-eval.yaml modified | All results |
| Assay version upgraded | All results |
| `assay cache clear` | All results |

---

## Storage Format

Cache data is stored in SQLite for reliability:

```sql
-- Simplified schema
CREATE TABLE cache_entries (
    id TEXT PRIMARY KEY,
    trace_fingerprint TEXT,
    policy_fingerprint TEXT,
    config_fingerprint TEXT,
    assay_version TEXT,
    result BLOB,
    created_at TIMESTAMP
);

CREATE INDEX idx_fingerprints ON cache_entries(
    trace_fingerprint, policy_fingerprint
);
```

---

## Troubleshooting

### Tests not re-running after changes

```bash
# Check what's cached
assay cache status --verbose

# Force re-run
assay run --config mcp-eval.yaml --no-cache
```

### Cache growing too large

```bash
# Check size
du -sh .assay/

# Clear old entries
assay cache clear --older-than 30d
```

### Inconsistent results between machines

Ensure all machines have:
- Same Assay version
- Same trace files
- Same policy files

Or use `--no-cache` for CI consistency.

---

## Best Practices

### 1. Commit Traces, Not Cache

```gitignore
# .gitignore
.assay/store.db
.assay/cache/
```

Traces are source-controlled; cache is ephemeral.

### 2. Use In-Memory for CI

```bash
assay run --db :memory:
```

Avoids cache state issues between CI runs.

### 3. Clear Cache After Major Changes

After upgrading Assay or restructuring policies:

```bash
assay cache clear
```

### 4. Monitor Cache Health

In long-running projects:

```bash
# Weekly check
assay validate --cache
```

---

## See Also

- [Traces](traces.md)
- [Replay Engine](replay.md)
- [CLI: assay run](../cli/run.md)
