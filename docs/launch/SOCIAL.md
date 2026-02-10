# Social Media Copy

## Twitter Thread

**Hook (Tweet 1):**
Stop shipping AI agents without runtime guards. 🛑

Most "Agent CI" is just LLM-as-a-judge vibing on outputs.
We need strict Policy-as-Code.

Meet Assay: Deterministic replay, eBPF enforcement, and audit-ready evidence for autonomous agents.

Written in Rust. Runs offline. 🧵👇
[Link to Repo]

**Tweet 2 (The Problem):**
Agents are non-deterministic.
Auditors hate non-determinism.

Assay solves this with a replay engine that mocks the world, not the agent.
Record once. Replay forever. Catch regressions before they merge.

[GIF: break-fix.gif]

**Tweet 3 (The Solution):**
What if your agent calls `delete_db` instead of `read_db`?

Assay intercepts every tool call at the runtime level.
Define policies in YAML. Enforce them in milliseconds.

No hallucinated tool calls allowed.

[GIF: sim.gif]

**Tweet 4 (CTA):**
Open source.
Privacy first (no telemetry).
Proves compliance (EU AI Act ready).

Try it in a Codespace right now:
[Link to Codespace]

GitHub: https://github.com/Rul1an/assay

## LinkedIn Post

**Topic:** AI Compliance & The "Black Box" Problem

How do you prove your AI agent followed the rules?

As we move from chatbots to autonomous agents, "vibes" aren't enough. We need evidence.

I've just open-sourced Assay to solve the runtime governance problem for agentic workflows.

It provides:
✅ **Deterministic Replay:** Catch regressions in CI.
✅ **Policy-as-Code:** strict runtime enforcement for MCP servers.
✅ **Evidence Bundles:** Cryptographic proof of what your agent did.

It's built in Rust, runs entirely offline, and produces machine-readable audit trails compatible with the upcoming EU AI Act requirements.

Check it out on GitHub: https://github.com/Rul1an/assay

#RustLang #AI #Security #Compliance #OpenSource #AgenticAI

## Reddit (r/rust)

**Title:** I built a runtime security tool for AI agents in Rust (eBPF + Deterministic Replay)

**Body:**
Hey Rustaceans,

I've been working on **Assay**, a CLI tool to secure autonomous AI agents.

Think of it as "Next-gen CI for Agents". It focuses on runtime enforcement and auditability rather than just "evals".

**Tech Stack:**
- **Rust** for the CLI and runner (obviously).
- **eBPF/LSM** hooks for enforcing file access policies at the kernel level (Linux).
- **JCS (RFC 8785)** for canonicalizing JSON evidence events to ensure deterministic hashing.
- **Tuirealm** for the TUI evidence explorer.

I'd love feedback on the crate structure or the eBPF integration!

Repo: https://github.com/Rul1an/assay
