# PR-3.4.1: Error Taxonomy + Actionable Diagnostics

## Summary

This PR introduces a comprehensive diagnostic system for Verdict with:
- **Stable error codes** (E001, E002, etc.) for documentation references
- **Actionable fix steps** (1-3 bullets per error)
- **Rich context** for debugging
- **Closest match hints** for trace misses using Levenshtein distance

## Files Changed

```
crates/verdict-core/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── errors/
    │   ├── mod.rs
    │   ├── diagnostic.rs    # Core diagnostic types
    │   └── similarity.rs    # String similarity for closest match
    └── trace/
        ├── mod.rs
        └── verify.rs        # Trace verification with hints

tests/acceptance/
├── trace_miss_hints.sh
├── dims_mismatch_actionable.sh
└── baseline_mismatch_actionable.sh
```

## Error Code Reference

| Code | Category | Description |
|------|----------|-------------|
| E001 | Trace | No matching trace entry found |
| E002 | Trace | Trace file not found |
| E003 | Trace | Trace schema invalid |
| E004 | Trace | Entry malformed (invalid JSON) |
| E020 | Baseline | Baseline file not found |
| E021 | Baseline | Suite name mismatch |
| E022 | Baseline | Schema version mismatch |
| E023 | Baseline | Fingerprint mismatch |
| E040 | Embedding | Dimensions mismatch |
| E041 | Embedding | Model ID mismatch |
| E042 | Embedding | Not precomputed for strict replay |
| E060 | Judge | Not precomputed for strict replay |
| E061 | Judge | Model mismatch |
| E062 | Judge | Disagreement (voting failed) |
| E080 | Config | Config file not found |
| E081 | Config | Parse error (invalid YAML) |
| E082 | Config | Validation error |
| E083 | Config | Unknown metric type |
| E100 | Runtime | Strict replay violation |
| E101 | Runtime | Rate limit exceeded |
| E102 | Runtime | Authentication failed |
| E103 | Runtime | Timeout |
| E120 | Database | Migration failed |
| E121 | Database | Database locked |
| E122 | Database | Database corrupted |

## Example Output

### Trace Miss with Closest Match

```
Error [E001] No matching trace entry for test 't1'

  Test: t1
  Expected: "What is the capital of France?"
  Closest:  "What is the capitol of France?" (similarity: 0.96)
            capital → capitol

Fix:
  1. Update your prompt template to match the trace entry
  2. Or re-record: verdict trace ingest --input <new-traces.jsonl>
  3. Or verify coverage: verdict trace verify --config <config.yaml>
```

### Embedding Dimensions Mismatch

```
Error [E040] Embedding dimensions mismatch

  Test: t2
  Expected dims: 1536 (model: text-embedding-3-small)
  Found dims: 3072 (model: text-embedding-3-large)

Fix:
  1. Re-precompute embeddings with model 'text-embedding-3-small'
  2. Run: verdict trace precompute-embeddings --model text-embedding-3-small
  3. Or update config to use model 'text-embedding-3-large'
```

### Baseline Suite Mismatch

```
Error [E021] Baseline suite mismatch

  Expected suite: prod-tests (schema v1)
  Found suite: dev-tests (schema v1)

Fix:
  1. Update config suite name to 'dev-tests' to match baseline
  2. Or regenerate baseline for suite 'prod-tests'
```

## Usage

### Creating Diagnostics

```rust
use verdict_core::errors::{Diagnostic, DiagnosticCode, DiagnosticContext, ClosestMatch};

// Trace miss with closest match
let diag = Diagnostic::new(
    DiagnosticCode::E001TraceMiss,
    "No matching trace entry for test 't1'",
    DiagnosticContext::TraceMiss {
        test_id: "t1".to_string(),
        expected_prompt: "What is the capital of France?".to_string(),
        closest_match: Some(ClosestMatch {
            prompt: "What is the capitol of France?".to_string(),
            similarity: 0.96,
            diff_positions: vec![DiffPosition {
                start: 16,
                end: 23,
                expected: "capital".to_string(),
                found: "capitol".to_string(),
            }],
        }),
    },
);

// Terminal output with ANSI colors
eprintln!("{}", diag.format_terminal());

// Plain text for logs/CI
eprintln!("{}", diag.format_plain());

// JSON for structured logging
println!("{}", serde_json::to_string(&diag).unwrap());
```

### Finding Closest Matches

```rust
use verdict_core::errors::{find_closest_match, similarity_score};

let candidates = vec![
    "What is the capitol of France?".to_string(),
    "What is the capital of Germany?".to_string(),
];

let closest = find_closest_match(
    "What is the capital of France?",
    &candidates,
    0.5, // minimum threshold
);

if let Some(match_) = closest {
    println!("Closest: {} (similarity: {:.2})", match_.prompt, match_.similarity);
    for diff in &match_.diff_positions {
        println!("  {} → {}", diff.expected, diff.found);
    }
}
```

### Trace Verification

```rust
use verdict_core::trace::{TraceVerifier, TestCase, TraceEntry};

let verifier = TraceVerifier::new()
    .with_similarity_threshold(0.7);

let diagnostics = verifier.verify_with_diagnostics(
    &tests,
    &traces,
    true,  // require_embeddings
    true,  // require_judge
);

for diag in diagnostics {
    eprintln!("{}", diag.format_terminal());
}
```

## Integration Points

### Runner Integration

In `engine/runner.rs`, wrap errors with diagnostics:

```rust
// Before
return Err(format!("No trace entry for prompt: {}", prompt));

// After
return Err(Diagnostic::new(
    DiagnosticCode::E001TraceMiss,
    format!("No matching trace entry for test '{}'", test.id),
    DiagnosticContext::TraceMiss {
        test_id: test.id.clone(),
        expected_prompt: prompt.clone(),
        closest_match: find_closest_match(&prompt, &trace_prompts, 0.5),
    },
));
```

### CLI Integration

In `cli/commands/ci.rs`, format diagnostics for output:

```rust
match result {
    Err(diag) => {
        if atty::is(atty::Stream::Stderr) {
            eprintln!("{}", diag.format_terminal());
        } else {
            eprintln!("{}", diag.format_plain());
        }
        std::process::exit(2);
    }
    Ok(_) => { /* ... */ }
}
```

## Acceptance Criteria

- [x] Trace miss shows closest match with similarity score
- [x] Trace miss shows diff positions (what changed)
- [x] Embedding dims mismatch shows both dims + model IDs
- [x] Embedding dims mismatch suggests fix command
- [x] Baseline mismatch shows suite/schema differences
- [x] All errors have 1-3 actionable fix steps
- [x] Errors serializable to JSON for structured logging
- [x] Unit tests for similarity functions
- [x] Unit tests for diagnostic formatting

## Testing

```bash
# Run unit tests
cargo test -p verdict-core

# Run acceptance tests
bash tests/acceptance/trace_miss_hints.sh
bash tests/acceptance/dims_mismatch_actionable.sh
bash tests/acceptance/baseline_mismatch_actionable.sh
```

## Mitigated Failure Modes

| # | Failure Mode | How This PR Helps |
|---|--------------|-------------------|
| 1 | Trace miss (prompt drift) | Closest match + diff highlighting |
| 3 | Schema/version drift | Clear error codes + context |
| 5 | Embedding dims mismatch | Shows both dims + model IDs |
| 10 | No "what to do next" | Every error has fix steps |
