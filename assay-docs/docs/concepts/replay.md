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
assay run --config mcp-eval.yaml --strict
```

In strict mode:
- Any policy violation fails the entire test
- Exit code is 1 if any test fails
- Ideal for blocking PRs with regressions

### Lenient Mode

Report violations but don't fail. Use for auditing.

```bash
assay run --config mcp-eval.yaml --lenient
```

In lenient mode:
- Violations are logged but don't fail
- Exit code is 0 even with violations
- Ideal for migration, baseline analysis

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
assay run --config mcp-eval.yaml
```

### Specify Trace File

```bash
# Run against a specific trace
assay run --config mcp-eval.yaml --trace-file traces/production-incident.jsonl
```

### Multiple Traces

```bash
# Run against all traces in a directory
assay run --config mcp-eval.yaml --trace-dir traces/
```

### In-Memory Database

For CI, skip disk writes:

```bash
assay run --config mcp-eval.yaml --db :memory:
```

---

## Replay with Debugging

### Verbose Output

```bash
assay run --config mcp-eval.yaml --verbose

# Output:
# [TRACE] Loading trace: traces/golden.jsonl
# [TRACE] Found 47 tool calls
# [REPLAY] Call 1: get_customer(id="123")
# [VALIDATE] args_valid: ✅ PASS
# [REPLAY] Call 2: update_customer(id="123", email="new@example.com")
# [VALIDATE] args_valid: ✅ PASS
# ...
```

### Step-by-Step

```bash
assay replay --trace traces/golden.jsonl --step

# Interactive mode:
# > [1/47] get_customer(id="123") — Press Enter to continue
# > [2/47] update_customer(...) — Press Enter to continue
```

### Export Replay Log

```bash
assay run --config mcp-eval.yaml --output-log replay.log
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

Suggestion: Validate trace with 'assay validate --trace <file>'
```

### Policy Mismatch

```
Warning: Tool 'new_feature' in trace not found in policy

The trace contains calls to 'new_feature', but no policy defines it.

Options:
  1. Add 'new_feature' to your policy file
  2. Use --ignore-unknown-tools to skip validation
  3. Use --strict to fail on unknown tools
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
    assay run \
      --config mcp-eval.yaml \
      --trace-file traces/golden.jsonl \
      --strict \
      --output sarif \
      --db :memory:
```

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | All tests passed |
| 1 | One or more tests failed |
| 2 | Configuration error |
| 3 | Trace file error |

---

## See Also

- [Traces](traces.md)
- [Cache & Fingerprints](cache.md)
- [CI Integration](../getting-started/ci-integration.md)
- [CLI: assay run](../cli/run.md)
