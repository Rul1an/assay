# Parity Testing: Batch vs Streaming

This directory contains the parity testing infrastructure for Assay v1.0.

## The "One Engine, Two Modes" Guarantee

Assay's core value proposition is:

> **Same policy + same input = same result, whether evaluated in batch or streaming mode.**

This guarantee is critical because:

1. **CI/CD reliability**: Developers can test in batch mode and trust that streaming will behave identically
2. **Debugging**: Reproduce streaming issues in batch mode for analysis
3. **Compliance**: Prove that your guardrails work the same everywhere

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    SHARED ENGINE                             │
│                   (assay-metrics)                            │
│                                                             │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────────────────┐ │
│  │ args_valid  │ │sequence_valid│ │ tool_blocklist         │ │
│  └──────┬──────┘ └──────┬──────┘ └───────────┬─────────────┘ │
│         │               │                     │              │
└─────────┼───────────────┼─────────────────────┼──────────────┘
          │               │                     │
    ┌─────┴─────┐   ┌─────┴─────┐         ┌─────┴─────┐
    │   BATCH   │   │   BATCH   │         │ STREAMING │
    │ (assay    │   │ (assay    │         │ (assay-   │
    │  run)     │   │  run)     │         │  mcp-     │
    │           │   │           │         │  server)  │
    └───────────┘   └───────────┘         └───────────┘
```

Both modes call the **same functions** in `assay-metrics`. The only difference is the execution context.

## Files

```
tests/
├── parity.rs           # Rust unit tests for parity
├── parity_suite.yaml   # YAML test suite (20 cases)
├── fp_suite.yaml       # False positive tests

.github/workflows/
└── parity.yml          # CI workflow
```

## Running Parity Tests

### Rust Unit Tests

```bash
# Run all parity tests
cargo test -p assay-core --test parity -- --nocapture

# Run specific test
cargo test -p assay-core --test parity test_args_valid_parity -- --nocapture
```

### YAML Suite

```bash
# Run the full parity suite
assay parity-test --suite tests/parity_suite.yaml

# Verbose output
assay parity-test --suite tests/parity_suite.yaml --verbose
```

## Test Coverage

| Check Type | Pass Cases | Fail Cases | Error Cases | Edge Cases |
|------------|------------|------------|-------------|------------|
| `args_valid` | 3 | 2 | 1 | 2 |
| `sequence_valid` | 2 | 2 | 1 | 0 |
| `tool_blocklist` | 2 | 2 | 1 | 2 |
| **Total** | **7** | **6** | **3** | **4** |

## Parity Verification Logic

```rust
pub fn verify_parity(check: &PolicyCheck, input: &CheckInput) -> ParityResult {
    let batch_result = batch::evaluate(check, input);
    let streaming_result = streaming::evaluate(check, input);

    let is_identical =
        batch_result.outcome == streaming_result.outcome
        && batch_result.reason == streaming_result.reason;

    ParityResult {
        batch_result,
        streaming_result,
        is_identical,
    }
}
```

For each test case, we:

1. Run the check in **batch mode** (simulating `assay run`)
2. Run the check in **streaming mode** (simulating `assay-mcp-server`)
3. Compare `outcome` (Pass/Fail/Error) and `reason` (explanation string)
4. **FAIL** the test if they differ

## What Causes Parity Violations?

Common causes of batch/streaming divergence:

| Issue | Example | Fix |
|-------|---------|-----|
| Different code paths | Batch uses regex, streaming uses string match | Share implementation |
| Floating point precision | `0.30000001 != 0.3` | Use consistent comparison |
| Serialization differences | JSON field order varies | Normalize before compare |
| Environment dependencies | Batch reads file, streaming has in-memory | Abstract I/O |
| Race conditions | Streaming has async timing | Make deterministic |

## CI Integration

The parity test is a **release gate**:

```yaml
# .github/workflows/parity.yml
- name: Run parity tests
  run: cargo test -p assay-core --test parity -- --nocapture

- name: Check result
  if: failure()
  run: |
    echo "PARITY TEST FAILED"
    echo "This is a RELEASE BLOCKER."
    exit 1
```

## Adding New Parity Tests

### In Rust

```rust
#[test]
fn test_new_check_parity() {
    let check = PolicyCheck {
        id: "my_new_check".into(),
        check_type: CheckType::ArgsValid,
        params: serde_json::json!({ /* ... */ }),
    };

    let input = CheckInput {
        tool_name: Some("MyTool".into()),
        args: Some(serde_json::json!({ /* ... */ })),
        trace: None,
    };

    let result = verify_parity(&check, &input);
    result.assert_parity();  // Panics if batch != streaming
    assert_eq!(result.batch_result.outcome, Outcome::Pass);
}
```

### In YAML

```yaml
- id: my_new_parity_test
  description: "Description of what we're testing"
  check_type: args_valid
  policy:
    schema:
      # ...
  input:
    tool_name: MyTool
    args:
      # ...
  expected:
    outcome: pass
    parity: must_match  # Required for all parity tests
```

## Debugging Parity Failures

When a parity test fails:

1. **Check the output:**
   ```
   PARITY VIOLATION for check 'my_check':
   Batch:     Fail - percent 50 exceeds maximum 30
   Streaming: Pass - args valid
   ```

2. **Find the divergence point:**
   - Is the outcome different? (Pass vs Fail)
   - Is the reason different? (Same outcome, different explanation)

3. **Trace the code paths:**
   - `batch::evaluate()` → which function is called?
   - `streaming::evaluate()` → which function is called?
   - Are they calling the same `shared::` function?

4. **Fix by unifying:**
   - Move logic to `shared::` module
   - Both modes should call the shared function

## Result Hashing

For CI caching and comparison, we compute a deterministic hash:

```rust
fn compute_result_hash(check_id: &str, outcome: &Outcome, reason: &str) -> String {
    let mut hasher = DefaultHasher::new();
    check_id.hash(&mut hasher);
    format!("{:?}", outcome).hash(&mut hasher);
    reason.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
```

Same inputs → same hash → parity verified.

## v1.0 Release Criteria

| Requirement | Target | Status |
|-------------|--------|--------|
| All parity tests pass | 100% | ⏳ |
| No known divergence | 0 issues | ⏳ |
| CI gate enabled | Required | ⏳ |
| Edge cases covered | 20+ tests | ✅ |

**Parity testing is a RELEASE BLOCKER for v1.0.**
