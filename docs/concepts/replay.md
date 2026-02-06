# Replay Engine

The replay engine is the core of Assay's zero-flake testing — deterministic re-execution without calling LLMs or tools.

---

## What is Replay?

**Replay** means re-executing an agent session using recorded behavior instead of live API calls:

```
Traditional Test:
  Prompt → LLM API → Tool Calls → Validation
  (slow, expensive, flaky)

Assay Replay:
  Trace → Replay Engine → Validation
  (instant, free, deterministic)
```

The replay engine reads a trace file and simulates the agent's execution, validating each step against your policies.

---

## How It Works

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│    Trace     │ ──► │   Replay     │ ──► │   Metrics    │
│  (recorded)  │     │   Engine     │     │  (validate)  │
└──────────────┘     └──────────────┘     └──────────────┘
                            │
                            ▼
                     ┌──────────────┐
                     │   Results    │
                     │  Pass/Fail   │
                     └──────────────┘
```

1. **Load Trace** — Read the recorded session (`.jsonl` file)
2. **Simulate Execution** — Process each tool call in order
3. **Validate** — Check arguments, sequences, blocklists
4. **Report** — Output pass/fail with detailed violations

---

## Replay Modes

### Strict Mode

Fail on any violation. Use for CI gates.

```bash
assay run --config eval.yaml --strict
```

In strict mode:
- Any policy violation fails the entire test
- Exit code is 1 if any test fails
- Ideal for blocking PRs with regressions

### Non-Strict Mode

Report violations but don't fail. Use for auditing.

```bash
assay run --config eval.yaml
```

Without `--strict`:
- Warn/flaky outcomes do not fail the process
- Exit code remains 0 unless blocking failures occur
- Useful for migration and exploratory audits

---

## Determinism Guarantees

Assay guarantees **identical results** on every run:

| Factor | Assay's Approach |
|--------|------------------|
| Random seeds | Fixed per trace |
| Timestamps | Normalized from trace |
| External calls | Mocked from trace data |
| Ordering | Preserved from recording |

This means:
- ✅ Same trace + same policies = same result, always
- ✅ No network variance
- ✅ No model variance
- ✅ No timing variance

---

## Replay vs. Live Execution

| Aspect | Replay | Live Execution |
|--------|--------|----------------|
| Speed | 1-10 ms | 1-30 seconds |
| Cost | $0.00 | $0.01-$1.00 |
| Determinism | 100% | 80-95% |
| Network | Not required | Required |
| Isolation | Complete | Shared state risks |

### When to Use Replay

- **CI/CD gates** — Every PR gets tested
- **Regression testing** — Catch breaking changes
- **Debugging** — Reproduce production incidents
- **Baseline comparison** — A vs. B testing

### When to Use Live

- **Development** — Exploring new features
- **E2E testing** — Full integration validation
- **Model evaluation** — Comparing LLM versions

---

## Running Replay

### Basic Replay

```bash
# Run all tests against the default trace
assay run --config eval.yaml
```

### Specify Trace File

```bash
# Run against a specific trace
assay run --config eval.yaml --trace-file traces/production-incident.jsonl
```

### Multiple Traces

```bash
# Run multiple traces by iterating files
for trace in traces/*.jsonl; do
  assay run --config eval.yaml --trace-file "$trace" --strict || exit $?
done
```

### In-Memory Database

For CI, skip disk writes:

```bash
assay run --config eval.yaml --db :memory:
```

---

## Replay with Debugging

### Detailed Explanation

```bash
assay explain --trace traces/golden.jsonl --policy policy.yaml --verbose

# Output:
# Step 1: get_customer(...)
# Verdict: Allowed
# Rules: args_valid, sequence_valid
# ...
```

### Bundle Replay

```bash
# Replay from an immutable replay bundle (offline by default)
assay replay --bundle .assay/bundles/run-123.tar.gz
```

### Export Explain Report

```bash
assay explain --trace traces/golden.jsonl --policy policy.yaml --format markdown --output replay.md
```

---

## Replay Isolation

Each replay is isolated:

- **No side effects** — Tools aren't actually called
- **No shared state** — Each run starts fresh
- **No external dependencies** — Works offline

This makes replay ideal for:
- Parallel test execution
- CI runners with no network
- Air-gapped environments

---

## Error Handling

### Trace Not Found

```
Error: Trace file not found: traces/missing.jsonl

Suggestion: Run 'assay import' first or check the path
```

### Invalid Trace Format

```
Error: Invalid trace format at line 15

  {"type":"tool_call","tool":"get_customer"}
                                           ^
  Missing required field: 'arguments'

Suggestion: Validate trace with 'assay trace verify --trace <file> --config eval.yaml'
```

### Policy Mismatch

```
Warning: Tool 'new_feature' in trace not found in policy

The trace contains calls to 'new_feature', but no policy defines it.

Options:
  1. Add 'new_feature' to your policy file
  2. Re-run with an updated policy file
  3. Validate config and trace coverage with `assay trace verify`
```

---

## Performance

Replay is fast because it:

1. **Skips network** — No HTTP calls
2. **Skips LLM inference** — No model computation
3. **Uses compiled validators** — Rust-native JSON Schema
4. **Caches fingerprints** — Skip unchanged traces

Typical performance:

| Trace Size | Replay Time |
|------------|-------------|
| 10 calls | ~1 ms |
| 100 calls | ~5 ms |
| 1000 calls | ~30 ms |

---

## CI Integration

### GitHub Actions

```yaml
- name: Run Assay Tests
  run: |
    assay ci \
      --config eval.yaml \
      --trace-file traces/golden.jsonl \
      --strict \
      --sarif .assay/reports/sarif.json \
      --junit .assay/reports/junit.xml \
      --db :memory:
```

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | All tests passed |
| 1 | One or more tests failed |
| 2 | Configuration/input error |
| 3 | Infrastructure/judge/provider error |

---

## See Also

- [Traces](traces.md)
- [Cache & Fingerprints](cache.md)
- [CI Integration](../getting-started/ci-integration.md)
- [CLI: assay run](../reference/cli/run.md)
