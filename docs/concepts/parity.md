# Parity Testing: Batch vs Streaming

Parity testing checks that the same policy logic behaves consistently across execution modes.

---

## Current State

There is no public `assay parity-test` CLI subcommand in the current binary.

Use repository test suites and CI workflows for parity verification:

```bash
# Example: run parity-focused Rust tests
cargo test -p assay-core --test parity -- --nocapture
```

---

## Why It Matters

- CI confidence: replay and runtime checks stay aligned.
- Debuggability: incidents found in one path can be reproduced in another.
- Compliance: a single policy intent produces consistent outcomes.

---

## Recommended Workflow

1. Add parity cases in Rust tests (or existing contract tests).
2. Run them in CI on pull requests.
3. Treat parity regressions as release blockers.

---

## See Also

- [Replay Engine](replay.md)
- [MCP Integration](../mcp/index.md)
