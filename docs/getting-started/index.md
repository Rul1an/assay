# Getting Started

Get Assay running in 5 minutes.

For the reproducible release contract, use the [release-pinned agent golden path](../guides/agent-golden-path.md). This page is the shorter human introduction.

## Overview

This guide covers:

1. [**Installation**](installation.md) — Install the Assay CLI
2. [**Quick Start**](quickstart.md) — Import a trace and run your first test
3. [**Your First Test**](first-test.md) — Write a custom policy from scratch
4. [**CI Integration**](ci-integration.md) — Add Assay to GitHub Actions / GitLab CI
5. [**Operator Proof Flow**](../guides/operator-proof-flow.md) — See trace ingest, shipped control-evidence linting, and proof-kit verification as one flow

---

## Prerequisites

- **Rust 1.96** for repository development, **Rust 1.89+** for public-crate
  source installs, or CPython 3.12 for Python SDK use
- CPython 3.12 on macOS x86_64/arm64 and Linux x86_64; other interpreters and platforms are not claimed.
- An MCP session log (or use our example)
- 5 minutes ☕

---

## The 60-Second Version

```bash
# Install
cargo install assay-cli --version 6.0.0 --locked

# Import an MCP session as trace
assay import --format inspector session.json --out-trace traces/session.jsonl

# Run tests
assay run --config eval.yaml --trace-file traces/session.jsonl

# Add to CI
# Copy the GitHub Action from ci-integration.md
```

This runs the recorded evaluation path. A clean result still depends on the supplied config and trace being complete.

---

## What You'll Learn

By the end of this guide, you'll understand:

| Concept | What it does |
|---------|--------------|
| **Traces** | Recorded agent behavior (the "golden" reference) |
| **Policies** | Rules that define correct behavior |
| **Metrics** | Functions that validate output |
| **Replay** | Deterministic re-execution without API calls |

---

## Next Steps

<div class="grid cards" markdown>

-   :material-download:{ .lg .middle } __Installation__

    ---

    Install the Assay CLI from a verified release channel.

    [:octicons-arrow-right-24: Install now](installation.md)

-   :material-rocket-launch:{ .lg .middle } __Quick Start__

    ---

    Run your first test in 60 seconds.

    [:octicons-arrow-right-24: Quick start](quickstart.md)

-   :material-shield-search:{ .lg .middle } __Operator Proof Flow__

    ---

    See one compact operator story: import, shipped pack evaluation, and offline release verification.

    [:octicons-arrow-right-24: Operator proof flow](../guides/operator-proof-flow.md)

</div>
