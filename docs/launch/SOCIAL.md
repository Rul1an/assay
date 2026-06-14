# Launch copy

Drafts for announcing Assay. Keep every claim bounded and accurate: describe what
Assay enforces and proves, and name what it does not claim. The technical
audiences these posts target reward precision and punish hype, so lead with the
engineering and the explicit non-claims, never with marketing.

**Claim guardrails (do not break these in any post):**

- Network egress enforcement is bounded to IPv4/TCP `connect` on Linux (eBPF/LSM
  and Landlock). Do not write "network enforcement" unqualified, and make no
  IP/CIDR, hostname, UDP, or QUIC claim.
- Evidence bundles are offline-verifiable records of what was observed. Say
  "evidence", not "cryptographic proof of what your agent did".
- An allow is the decision to forward a call, not proof the side effect happened.
- Assay is not a prompt-injection or tool-poisoning detector, not a trust score,
  and makes no compliance claim. Evidence lint packs are technical checks, never
  a "compliant" or "ready" claim.

---

## Show HN

**Title:** Show HN: Assay – policy-as-code for MCP agents (Rust, eBPF, offline)

**Body:**

I kept hitting the same gap with MCP agents: you can eval the model's outputs all
day, but nothing sits between the agent and the tool call and decides, before the
call runs, whether it is allowed. So I built Assay.

It is a fail-closed proxy in front of an MCP server. Each `tools/call` is checked
against a YAML policy and denied before it runs if it does not match. Allowed
calls are recorded into an offline-verifiable evidence bundle, so you can replay
and audit what actually ran later without trusting a dashboard.

On Linux it also enforces network egress in the kernel via eBPF/LSM and Landlock,
bounded to IPv4/TCP `connect`: a connect to a non-allowlisted port is denied with
EACCES, and there is a per-run real-block check that proves the deny happened.

The part I care about most is the discipline around claims. Assay is explicit
about what it does not do: it is not a prompt-injection or tool-poisoning
detector, it does not emit a trust score, and an allow is the decision to forward
a call, not proof the side effect happened. Every evidence claim is labelled by
how it is backed (verified / self-reported / inferred / absent).

Rust, MIT, no hosted backend, no telemetry. Feedback on the policy model and the
eBPF/LSM path especially welcome.

https://github.com/Rul1an/assay

---

## Reddit — r/rust

**Title:** Assay: a policy-as-code gate for AI agents, in Rust (eBPF/LSM + deterministic replay)

**Body:**

Hey Rustaceans,

I have been building **Assay**, a CLI that puts a policy boundary in front of
Model Context Protocol (MCP) agents. It decides each tool call against a YAML
policy before it runs, records an offline-verifiable evidence bundle of what
executed, and on Linux enforces IPv4/TCP egress in the kernel.

Stack notes that might interest this sub:

- Rust for the CLI, proxy, and runner.
- eBPF/LSM hooks plus Landlock for kernel-level file-access and IPv4/TCP-connect
  egress enforcement (Linux). The egress path is fail-closed: a policy it cannot
  express as an explicit port allowlist is refused, not partially applied.
- JCS (RFC 8785) to canonicalize the JSON evidence events for deterministic
  content hashing.
- A ratatui TUI for browsing evidence bundles offline.

It is deliberately bounded: the kernel enforcement is scoped to IPv4/TCP connect,
and it makes no UDP/QUIC/CIDR claim. Happy to go into the crate layout or the
eBPF integration. Feedback welcome.

Repo: https://github.com/Rul1an/assay

---

## Reddit — r/netsec / MCP communities

**Title:** Assay: fail-closed policy enforcement and evidence for MCP tool calls

**Body:**

With the run of MCP CVEs this year (tool poisoning, rug pulls, over-broad tool
access), I wanted a control that acts at the tool-call boundary rather than after
the fact. Assay is a fail-closed MCP proxy: it denies a `tools/call` before it
runs unless a YAML policy allows it, and it records each decision into an
offline-verifiable evidence bundle.

On Linux it adds kernel-level egress enforcement via eBPF/LSM and Landlock,
bounded to IPv4/TCP `connect`, with a per-run real-block check.

It is scoped on purpose and honest about it: it is a gate and an evidence layer,
not a prompt-injection or tool-poisoning detector and not a trust score. Each
evidence claim carries how it is backed.

Open source, Rust, MIT, runs offline. https://github.com/Rul1an/assay

---

## dev.to

**Title:** Putting a policy boundary in front of MCP agents

A short, technical walkthrough rather than an announcement: the problem (no
decision point before an MCP tool call runs), the approach (a fail-closed proxy +
offline-verifiable evidence + bounded kernel egress enforcement on Linux), a
copy-paste `assay mcp wrap` quickstart, and an explicit "what this does not
claim" section. Link to the repo and the MCP quickstart. Keep it engineering-led;
no marketing framing.

---

## Twitter / X (optional, bounded)

Single post, no thread theatrics:

> Assay: policy-as-code for MCP agents. A fail-closed proxy that denies risky
> tool calls before they run, records offline-verifiable evidence of what ran,
> and enforces IPv4/TCP egress in the kernel on Linux (eBPF/LSM, Landlock).
> Rust, MIT, offline. Bounded by design. https://github.com/Rul1an/assay

---

## LinkedIn (optional, bounded)

> I open-sourced Assay, a policy boundary for MCP agents.
>
> It decides each tool call against a YAML policy before it runs, records an
> offline-verifiable evidence bundle of what executed, and on Linux enforces
> IPv4/TCP egress in the kernel via eBPF/LSM and Landlock.
>
> It is deliberately bounded: a gate and an evidence layer, not a detector and
> not a trust score. Every evidence claim is labelled by how it is backed.
>
> Rust, MIT, offline-first. https://github.com/Rul1an/assay
