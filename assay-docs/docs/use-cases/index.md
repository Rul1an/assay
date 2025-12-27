# Use Cases

Real-world scenarios where Assay shines.

---

## Overview

Assay is designed for teams building production AI agents. Here are the most common use cases:

<div class="grid cards" markdown>

-   :material-source-pull:{ .lg .middle } __CI Regression Gate__

    ---

    Catch breaking changes before they hit production. Every PR gets validated.

    [:octicons-arrow-right-24: Learn more](ci-gate.md)

-   :material-bug:{ .lg .middle } __Trace-Driven Debugging__

    ---

    Reproduce and diagnose production failures using recorded traces.

    [:octicons-arrow-right-24: Learn more](debugging.md)

-   :material-shield-lock:{ .lg .middle } __Air-Gapped Enterprise__

    ---

    Run evaluations in secure environments with no external network access.

    [:octicons-arrow-right-24: Learn more](air-gapped.md)

-   :material-robot:{ .lg .middle } __Agent Self-Correction__

    ---

    Let agents validate their own actions before executing them.

    [:octicons-arrow-right-24: Learn more](self-correction.md)

</div>

---

## Quick Comparison

| Use Case | Key Benefit | Typical User |
|----------|-------------|--------------|
| CI Regression Gate | Zero-flake tests | DevOps, Platform |
| Trace-Driven Debugging | Fast root cause analysis | On-call Engineer |
| Air-Gapped Enterprise | Compliance, privacy | Security, FinTech |
| Agent Self-Correction | Runtime guardrails | Agent Developer |

---

## By Industry

### Financial Services

- **Requirement:** No data can leave the network
- **Solution:** Air-gapped deployment with local-only evaluation
- **Metrics:** Sequence validation (auth before transactions)

### Healthcare

- **Requirement:** HIPAA compliance, audit trails
- **Solution:** Trace recording + policy enforcement
- **Metrics:** Blocklist (no unauthorized data access)

### E-commerce

- **Requirement:** Prevent pricing/discount errors
- **Solution:** Argument validation on business-critical tools
- **Metrics:** args_valid with min/max constraints

### SaaS Platforms

- **Requirement:** Fast iteration without breaking things
- **Solution:** CI gates on every PR
- **Metrics:** Full test suite in milliseconds

---

## Getting Started

1. **Identify your pain point** — Flaky tests? Slow CI? Compliance needs?
2. **Pick a use case** — Start with one, expand later
3. **Follow the guide** — Each use case has step-by-step instructions
4. **Measure results** — Track time saved, failures caught

---

## See Also

- [Quick Start](../getting-started/quickstart.md)
- [Core Concepts](../concepts/index.md)
- [MCP Integration](../mcp/index.md)
