<p align="center">
  <h1 align="center">Assay</h1>
  <p align="center">
    <strong>CI-native evidence compiler for MCP and A2A governance</strong><br />
    <span>Deterministic policy enforcement, canonical evidence, and reviewable trust artifacts for agent systems.</span>
  </p>
  <p align="center">
    <a href="https://crates.io/crates/assay-cli"><img src="https://img.shields.io/crates/v/assay-cli.svg" alt="Crates.io"></a>
    <a href="https://github.com/Rul1an/assay/actions/workflows/ci.yml"><img src="https://github.com/Rul1an/assay/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
    <a href="https://github.com/Rul1an/assay/blob/main/LICENSE"><img src="https://img.shields.io/crates/l/assay-core.svg" alt="License"></a>
  </p>
  <p align="center">
    <a href="#see-it-work">See It Work</a> ·
    <a href="examples/mcp-quickstart/">Quick Start</a> ·
    <a href="docs/guides/github-action.md">CI Guide</a> ·
    <a href="https://github.com/Rul1an/assay/discussions">Discussions</a>
  </p>
</p>

---

Your MCP agent calls `read_file`, `exec`, `web_search` — but should it, and what can you honestly **prove** about that run afterward?

**Assay compiles agent runtime signals into enforceable decisions and portable evidence artifacts.** The wedge is familiar: sit between the agent and MCP servers, **allow or deny** tool calls from policy, and record every decision. The product story is broader: canonical **evidence**, **bounded trust claims** (what is verified vs merely visible), and outputs you can hand to CI, security review, or audit — without a hosted backend.

**Positioning:** Assay is best understood as a **CI-native protocol-governance layer**: canonical evidence compiler + protocol-aware policy checks. It is **not** a trust-score engine, a generic eval dashboard, or an observability product with a thin security veneer.

| | |
|---|---|
| **Enforce** | Intercept MCP tool calls, apply policy, **ALLOW** / **DENY** deterministically. |
| **Compile** | Turn traces, decisions, and bundles into **canonical evidence** — not raw OTel or ad hoc logs as truth. |
| **Prove** | Export **tamper-evident bundles**, **Trust Basis** (`trust-basis.json`), **Trust Card** (`trustcard.json` / `trustcard.md`), SARIF, and CI gates. |

No hosted backend. No API keys for core flows. **Deterministic** — same input, same decision, every time.

> **Trust Compiler line:** Release **v3.7.0** is the prepared three-family evidence-portability line. It carries forward **v3.3.0** as the first release that shipped **both** built-in evidence lint companion packs (`mcp-signal-followup`, `a2a-signal-followup`), **v3.4.0** as the public line for **`G4-A` Phase 1** (`payload.discovery`), built-in **`P2c`** (`a2a-discovery-card-followup`), **`K1-A` Phase 1** (`payload.handoff`), **`v3.5.0`** as the first public release of **`K2-A` Phase 1** (`episode_start.meta.mcp.authorization_discovery`), **`v3.5.1`** as the official-MCP-Registry publication foundation for `assay-mcp-server`, and **`v3.6.0`** as the first external-eval receipt lane for Promptfoo assertion-component results. **`v3.7.0`** adds claim-visible runtime decision and model inventory/provenance receipt families, starting with OpenFeature boolean `EvaluationDetails` and CycloneDX ML-BOM model components. Pack YAML still distinguishes the substrate floor `>=3.2.3` from the G4-A / P2c floor `>=3.3.0` — see [MIGRATION — Trust Compiler 3.2](docs/architecture/MIGRATION-TRUST-COMPILER-3.2.md).

> **Repository truth:** release notes and [CHANGELOG.md](CHANGELOG.md) remain the authority for what is actually public. `main` may carry release-prep commits before a tag is cut; crates.io publication is separate from repository merge state.

```
  Agent ──► Assay ──► MCP Server
              │
              ├─ ✅ ALLOW / ❌ DENY  (policy)
              ├─► 📋 Evidence bundle (verifiable)
              └─► 📊 Trust Basis → Trust Card → SARIF / CI
```

> **CLI:** The `mcp` command group is **hidden** from top-level `assay --help` while the surface stabilizes; it is supported. Use `assay mcp --help`, `assay mcp wrap …`, or follow the [MCP Quickstart](examples/mcp-quickstart/).

> **Wedge, not category.** “MCP firewall” describes the control plane; **trust compilation** describes the outcome: reviewable claims backed by evidence. See [ADR-033](docs/architecture/ADR-033-OTel-Trust-Compiler-Positioning.md) and [RFC-005](docs/architecture/RFC-005-trust-compiler-mvp-2026q2.md).

## See It Work

[![SafeSkill 72/100](https://img.shields.io/badge/SafeSkill-72%2F100_Passes%20with%20Notes-yellow)](https://safeskill.dev/scan/rul1an-assay)

```bash
cargo install assay-cli

mkdir -p /tmp/assay-demo && echo "safe content" > /tmp/assay-demo/safe.txt

assay mcp wrap --policy examples/mcp-quickstart/policy.yaml \
  -- npx @modelcontextprotocol/server-filesystem /tmp/assay-demo
```

```
✅ ALLOW  read_file  path=/tmp/assay-demo/safe.txt  reason=policy_allow
✅ ALLOW  list_dir   path=/tmp/assay-demo/           reason=policy_allow
❌ DENY   read_file  path=/tmp/outside-demo.txt      reason=path_constraint_violation
❌ DENY   exec       cmd=ls                          reason=tool_denied
```

Inspect the audit artifact:

```bash
assay evidence show demo/fixtures/bundle.tar.gz
```

![Evidence Bundle Inspector](demo/output/screenshots/evidence-bundle-inspector.svg)

The bundle is tamper-evident and cryptographically verifiable. Signed mandate events can include an Ed25519-backed authorization trail for high-risk actions.

### Trust artifacts from a verified bundle

Install from [crates.io](https://crates.io/crates/assay-cli) or source (`cargo install --path crates/assay-cli`), then:

```bash
# Machine-readable claim basis (deterministic, claim-first)
assay trust-basis generate demo/fixtures/bundle.tar.gz > trust-basis.json

# Human + machine Trust Card (schema v5 — ten trust claims; key by `id`, not row count)
assay trustcard generate demo/fixtures/bundle.tar.gz --out-dir ./trust-out
# → trust-out/trustcard.json , trust-out/trustcard.md
```

`trust-basis.json` emits claims from a bounded, versioned vocabulary for this schema (examples: `bundle_verified`, `delegation_context_visible`, `authorization_context_visible`, `containment_degradation_observed`, `external_eval_receipt_boundary_visible`, `external_decision_receipt_boundary_visible`, `external_inventory_receipt_boundary_visible`, …). Claim `id` values are stable across runs, but consumers **must not** rely on row count or ordering; always key by `id`. It is **not** a scalar trust score. The Trust Card is a deterministic render of the same claim rows plus frozen non-goals. **Contract versions, pack floors, and release checklist:** [docs/architecture/MIGRATION-TRUST-COMPILER-3.2.md](docs/architecture/MIGRATION-TRUST-COMPILER-3.2.md), [docs/reference/receipt-family-matrix.json](docs/reference/receipt-family-matrix.json).

In the `v3.7.0` line, supported external eval outcomes, runtime decision details, and model inventory/provenance surfaces can enter this compiler path as bounded receipts rather than full upstream truth. The first three claim-visible families are Promptfoo assertion-component results, OpenFeature boolean `EvaluationDetails`, and CycloneDX ML-BOM model components; [From Promptfoo JSONL to Evidence Receipts](docs/notes/FROM-PROMPTFOO-JSONL-TO-EVIDENCE-RECEIPTS.md) explains the first lane.

## What you get

| Output | Role |
|--------|------|
| **Policy gate** | MCP `wrap` — deterministic allow/deny before tools run (see CLI note above the diagram). |
| **Evidence bundle** | Offline-verifiable, tamper-evident archive for audit and replay. |
| **External receipts** | `v3.7.0` line: selected eval outcomes, runtime decision details, and inventory/provenance surfaces as bounded evidence receipts. |
| **Trust Basis** | Canonical `trust-basis.json` — bounded claim classification from verified bundles. |
| **Trust Card** | `trustcard.json` / `trustcard.md` — same claims, review-friendly artifact. |
| **SARIF / CI** | GitHub Action, Security tab integration, policy gates on PRs. |

## Evidence levels (trust vocabulary)

Trust claims use explicit **epistemology**, not a single “safety score”:

| Level | Meaning |
|-------|---------|
| `verified` | Backed by direct evidence or offline verification in the bundle/path |
| `self_reported` | Emitted by the system without stronger independent corroboration |
| `inferred` | Derived from bounded, documented rules |
| `absent` | No trustworthy evidence supports the claim |

Assay does **not** ship a primary aggregate trust score or a `safe/unsafe` badge as the main output. See [ADR-033](docs/architecture/ADR-033-OTel-Trust-Compiler-Positioning.md).

## Is This For Me?

**Yes, if you:**
- Build with Claude Desktop, Cursor, Windsurf, or any MCP client
- Ship agents that call tools and you need to control which ones
- Want a CI gate that catches tool-call regressions before production
- Need **bounded auditability and trust artifacts**, not only sampled observability

**Not yet, if you:**
- Don't use MCP (Assay is MCP-native; other protocols use adapters)
- Need a hosted dashboard (Assay is CLI-first and offline)
- Want a magic trust score or badge as the main output

## Add to Cursor in 30 Seconds

Assay ships a helper that finds your local Cursor MCP config path and prints a ready-to-paste entry:

```bash
assay mcp config-path cursor
```

It generates JSON like:

```json
{
  "filesystem-secure": {
    "command": "assay",
    "args": [
      "mcp",
      "wrap",
      "--policy",
      "/path/to/policy.yaml",
      "--",
      "npx",
      "-y",
      "@modelcontextprotocol/server-filesystem",
      "/Users/you"
    ]
  }
}
```

The same wrapped command works in other MCP clients — see [MCP Quick Start](docs/mcp/quickstart.md).

## Policy Is Simple

```yaml
version: "2.0"
name: "my-policy"

tools:
  allow: ["read_file", "list_dir"]
  deny: ["exec", "shell", "write_file"]

schemas:
  read_file:
    type: object
    additionalProperties: false
    properties:
      path:
        type: string
        pattern: "^/app/.*"
        minLength: 1
    required: ["path"]
```

Legacy `constraints:` policies still work. Use `assay policy migrate` for the v2 JSON Schema form, or `assay init --from-trace trace.jsonl` to generate from observed behavior.

See [Policy Files](docs/reference/config/policies.md).

## OpenTelemetry In, Canonical Evidence Out

Assay ingests OpenTelemetry JSONL, builds replayable traces, and exports **canonical evidence** — OTel is a bridge, not the sole semantic authority.

```bash
assay trace ingest-otel \
  --input otel-export.jsonl \
  --db .eval/eval.db \
  --out-trace traces/otel.v2.jsonl
```

See [OpenTelemetry & Langfuse](docs/guides/otel-langfuse.md).

## Add to CI

```yaml
# .github/workflows/assay.yml
name: Assay Gate
on: [push, pull_request]
permissions:
  contents: read
  security-events: write
jobs:
  assay:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: Rul1an/assay-action@v2
```

PRs that violate policy get blocked; SARIF can surface in the Security tab.

## Why Assay (trust compiler)

| | |
|---|---|
| **Canonical evidence** | Assay’s evidence model is the stable contract; OTel and adapters map into it. |
| **Deterministic** | Same input, same decision — not probabilistic. |
| **Portable artifacts** | Bundles, Trust Basis, Trust Card, SARIF — for CI, review, audit. |
| **Bounded claims** | Explicit about what is **verified** vs **visible** vs **absent** — no score-first UX. |
| **MCP-native wedge** | `assay mcp wrap` is the fast path (the `mcp` group is hidden from `assay --help`; use `assay mcp --help`). Adapters extend the same engine. |
| **Offline-first** | No backend required for core enforcement and bundle verification. |

## Beyond MCP: Protocol Adapters

Assay ships adapters that map protocol events into **canonical evidence** (same policy and evidence story, different transports):

| Protocol | Adapter | What it maps |
|----------|---------|--------------|
| **ACP** (OpenAI/Stripe) | `assay-adapter-acp` | Checkout events, payment intents, tool calls |
| **A2A** (Google) | `assay-adapter-a2a` | Agent capabilities, task delegation, artifacts |
| **UCP** (Google/Shopify) | `assay-adapter-ucp` | Discover/buy/post-purchase state transitions |

Adapter crates are **workspace / binary–driven** (not published as separate `crates.io` packages); consume them via this repo or released **assay** builds.

Governance stays protocol-agnostic; **the evidence and claim layer stays the same** as protocols evolve.

## Measured Latency

On the M1 Pro/macOS fragmented-IPI harness, protected tool-decision path:

- Main protection run: `0.771ms` p50 / `1.913ms` p95
- Fast-path scenario: `0.345ms` p50 / `1.145ms` p95

These are tool-decision timings, not end-to-end model latency. (See [Research & experiments](#research-mappings-experiments) for methodology context.)

## Install

```bash
cargo install assay-cli
```

CI: [GitHub Action](https://github.com/marketplace/actions/assay-ai-agent-security). Python SDK: `pip install assay-it`

## Learn More

- [MCP Quickstart](examples/mcp-quickstart/) — filesystem server walkthrough
- [Policy Files](docs/reference/config/policies.md) — YAML schema for `assay mcp wrap`
- [OpenTelemetry & Langfuse](docs/guides/otel-langfuse.md) — traces → replay and evidence
- [CI Guide](docs/guides/github-action.md) — GitHub Action
- [Evidence Store](docs/guides/evidence-store-aws-s3.md) — S3, B2, MinIO
- [ADR-033: Trust compiler positioning](docs/architecture/ADR-033-OTel-Trust-Compiler-Positioning.md)
- [RFC-005: Trust compiler MVP & Trust Card](docs/architecture/RFC-005-trust-compiler-mvp-2026q2.md)

## Research, mappings & experiments

**Bounded context:** numbers below support **mapping and experiments**, not a product “security score.”

- [OWASP MCP Top 10 Mapping](docs/security/OWASP-MCP-TOP10-MAPPING.md) — how Assay relates to each risk category (coverage is **not** a scalar guarantee).
- Third-party survey: popular MCP servers often show weak defaults — Assay adds policy + evidence; see discussion in the mapping doc.
- [Security experiments](docs/architecture/SYNTHESIS-TRUST-CHAIN-TRIFECTA-2026q2.md) — attack vectors and harness notes (methodology matters more than headline counts).

## Contributing

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

See [CONTRIBUTING.md](CONTRIBUTING.md). **Discussions:** [GitHub Discussions](https://github.com/Rul1an/assay/discussions) — seed topics for pinned threads live in [docs/community/DISCUSSIONS.md](docs/community/DISCUSSIONS.md).

## License

[MIT](LICENSE)
