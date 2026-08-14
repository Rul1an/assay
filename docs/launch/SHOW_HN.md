# Show HN Draft

**Title:** Show HN: Assay – Policy-as-Code for AI agents (deterministic replay, evidence bundles)

**Body:**

Hi HN,

I've been building Assay to solve a problem I kept hitting: how do you test autonomous AI agents deterministically in CI, and prove to auditors what they actually did?

Most "agent CI" tools today are focused on evals (LLM-as-a-judge) or observability. Assay focuses on **runtime security and auditability**.

Core loop:
1.  **Record** agent traces (MCP transcripts, API calls).
2.  **Generate** policies automatically from observed behavior ("Learning Mode").
3.  **Replay** deterministically in CI — same trace + same flags = identical outcome.
4.  **Produce** evidence bundles for compliance (EU AI Act, SOC2).
5.  **Simulate** attacks (prompt injection, tool abuse) to prove your gates actually work.

It's written in Rust. It runs offline. No telemetry. No vendor lock-in. No signup.

The evidence bundle format uses content-addressed events (JCS canonicalization, SHA-256, and a deterministic run-root digest). A verifier can recompute whether carried bytes match the recorded manifest. That does not prove an external side effect or provider outcome.

Repo: https://github.com/Rul1an/assay

Happy to answer any questions about the eBPF enforcement or the deterministic replay engine!
