# Getting Started

Get Assay running in 5 minutes.

## Overview

This guide covers:

1. [**Installation**](installation.md) — Install the Assay CLI
2. [**Quick Start**](quickstart.md) — Import a trace and run your first test
3. [**Your First Test**](first-test.md) — Write a custom policy from scratch
4. [**CI Integration**](ci-integration.md) — Add Assay to GitHub Actions / GitLab CI

---

## Prerequisites

- **Rust 1.70+** or **Python 3.9+**
- An MCP session log (or use our example)
- 5 minutes ☕

---

## The 60-Second Version

```bash
# Install
pip install assay

# Import an MCP session (creates config automatically)
assay import --format mcp-inspector session.json --init

# Run tests
assay run --config mcp-eval.yaml

# Add to CI
# Copy the GitHub Action from ci-integration.md
```

That's it. Your AI agent now has zero-flake regression tests.

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

    Install Assay via pip, cargo, or Docker.

    [:octicons-arrow-right-24: Install now](installation.md)

-   :material-rocket-launch:{ .lg .middle } __Quick Start__

    ---

    Run your first test in 60 seconds.

    [:octicons-arrow-right-24: Quick start](quickstart.md)

</div>
