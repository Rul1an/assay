<p align="center">
  <h1 align="center">Assay</h1>
  <p align="center">
    <strong>Evidence compiler for agent review artifacts</strong><br />
    <span>Portable evidence receipts, verifiable bundles, and bounded Trust Basis claims for agent systems.</span>
  </p>
  <p align="center">
    <a href="https://crates.io/crates/assay-cli"><img src="https://img.shields.io/crates/v/assay-cli.svg" alt="Crates.io"></a>
    <a href="https://github.com/Rul1an/assay/actions/workflows/ci.yml"><img src="https://github.com/Rul1an/assay/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
    <a href="https://github.com/Rul1an/assay/blob/main/LICENSE"><img src="https://img.shields.io/crates/l/assay-core.svg" alt="License"></a>
  </p>
  <p align="center">
    <a href="#see-it-work">See It Work</a> ·
    <a href="docs/use-cases/evidence-receipts-from-promptfoo-jsonl.md">Promptfoo JSONL</a> ·
    <a href="docs/use-cases/openfeature-evaluationdetails-to-ci-review-artifact.md">OpenFeature</a> ·
    <a href="docs/use-cases/cyclonedx-mlbom-model-to-inventory-receipt.md">CycloneDX ML-BOM</a> ·
    <a href="examples/mcp-quickstart/">Quick Start</a> ·
    <a href="docs/guides/github-action.md">CI Guide</a> ·
    <a href="https://github.com/Rul1an/assay/discussions">Discussions</a>
  </p>
</p>

---

Use Assay if you already have machine-readable AI outcomes or agent tool-call tests and want a small reviewable artifact boundary in CI.

Start with the path that matches what you already have:

| You have | Use this when | What you get | Next click |
|---|---|---|---|
| Promptfoo JSONL from CI evals | You want smaller PR evidence than a full eval export | Eval outcome receipts, verified bundle, Trust Basis diff | [Promptfoo JSONL](docs/use-cases/evidence-receipts-from-promptfoo-jsonl.md) |
| OpenFeature boolean `EvaluationDetails` | You want CI evidence for a runtime flag decision boundary | Decision receipt, verified bundle, Trust Basis diff | [OpenFeature EvaluationDetails](docs/use-cases/openfeature-evaluationdetails-to-ci-review-artifact.md) |
| CycloneDX ML-BOM model component | You want CI evidence for the model inventory/provenance boundary that existed | Inventory receipt, verified bundle, Trust Basis diff | [CycloneDX ML-BOM](docs/use-cases/cyclonedx-mlbom-model-to-inventory-receipt.md) |
| MCP tool calls | You are ready to put a policy file around tool execution | Allow/deny audit trail and evidence for observed tool behavior | [MCP Quick Start](examples/mcp-quickstart/) |
| A GitHub PR gate | You want CI to block regressions from checked artifacts | Trust Basis diff, gate status, SARIF/JUnit-ready output | [CI Guide](docs/guides/github-action.md) |

The core workflow is intentionally small: import or record a bounded outcome, bundle and verify it, compile `trust-basis.json`, then gate the Trust Basis diff. Assay does not make the upstream tool the source of truth; it makes the evidence boundary inspectable.

```text
Trust Basis Gate
Status: OK
Bundles verified: 1
Regressed claims: 0
```

Assay is not a trust-score engine, a generic eval dashboard, or a hosted observability product. See [What Assay is and is not](docs/concepts/scope.md) for the boundary.

## Is This For Me?

**Yes, if you:**
- already have eval output, runtime decisions, inventory artifacts, or MCP tool-call tests
- want a CI review artifact instead of a dashboard-only result
- need bounded auditability, not a scalar trust badge

**Not yet, if you:**
- need Assay to judge model correctness or policy quality for you
- want a hosted dashboard as the primary product
- want a compliance claim instead of a bounded evidence boundary

## Install

```bash
cargo install assay-cli
```

CI: [GitHub Action](https://github.com/marketplace/actions/assay-ai-agent-security). Python SDK: `pip install assay-it`.

No hosted backend. No API keys for core flows. Deterministic: same input, same decision.

> **CLI release note:** `main` currently includes a small CLI UX close-out for
> the `v3.13.0` release line: `assay run --format json` emits machine-readable run
> results to stdout, `model: trace` now errors clearly when `--trace-file` is
> missing, `assay validate eval.yaml` works beside `--config eval.yaml`, and
> `assay trust-card` is the canonical Trust Card command spelling. MCP runtime
> commands are grouped under `assay mcp`, with hidden compatibility shims for
> the previous flat `assay discover`, `assay kill`, and `assay tool ...` paths.
> These are merged on `main`; see [CHANGELOG.md](CHANGELOG.md) for release
> status before assuming crates.io availability.

<details>
<summary>Evidence levels and non-goals</summary>

Trust claims use explicit **epistemology**, not a single “safety score”:

| Level | Meaning |
|-------|---------|
| `verified` | Backed by direct evidence or offline verification in the bundle/path |
| `self_reported` | Emitted by the system without stronger independent corroboration |
| `inferred` | Derived from bounded, documented rules |
| `absent` | No trustworthy evidence supports the claim |

Assay does **not** ship a primary aggregate trust score or a `safe/unsafe` badge as the main output. See [ADR-033](docs/architecture/ADR-033-OTel-Trust-Compiler-Positioning.md).

</details>

## What ships today

| Output | Role |
|--------|------|
| **Policy gate** | MCP `wrap` — deterministic allow/deny before tools run (see CLI note below the diagram). |
| **Evidence bundle** | Offline-verifiable, tamper-evident archive for audit and replay. |
| **External receipts** | Selected eval outcomes, runtime decision details, and inventory/provenance surfaces as bounded evidence receipts with JSON Schema contracts. |
| **Trust Basis** | Canonical `trust-basis.json` — bounded claim classification from verified bundles. |
| **Trust Card** | `trustcard.json` / `trustcard.md` / `trustcard.html` — same claims, review-friendly artifacts. |
| **SARIF / CI** | GitHub Action, Security tab integration, policy gates on PRs. |

> **Repository truth:** release notes and [CHANGELOG.md](CHANGELOG.md) remain the authority for what is actually public. `main` may carry release-prep commits before a tag is cut; crates.io publication is separate from repository merge state.

```
  Agent ──► Assay ──► MCP Server
              │
              ├─ ✅ ALLOW / ❌ DENY  (policy)
              ├─► 📋 Evidence bundle (verifiable)
              └─► 📊 Trust Basis → Trust Card → SARIF / CI
```

> **CLI:** MCP runtime commands live under `assay mcp`. Use `assay mcp --help`,
> `assay mcp wrap …`, `assay mcp discover`, `assay mcp kill`, or follow the
> [MCP Quickstart](examples/mcp-quickstart/).

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

After a bundle verifies, compile the claim artifact:

```bash
# Machine-readable claim basis (deterministic, claim-first)
assay trust-basis generate demo/fixtures/bundle.tar.gz > trust-basis.json
```

`trust-basis.json` is the canonical output for CI and review. Claim `id` values are stable across runs; consumers should key by `id`, not row count or order. It is not a scalar trust score.

The current claim-visible receipt families are Promptfoo assertion-component results, OpenFeature boolean `EvaluationDetails`, and CycloneDX ML-BOM model components. See the [receipt-family matrix](docs/reference/receipt-family-matrix.json), the [three-family note](docs/notes/EVIDENCE-RECEIPTS-FOR-AI-OUTCOMES-RUNTIME-DECISIONS-MODEL-INVENTORY.md), and [Evidence Receipts in Action](docs/notes/EVIDENCE-RECEIPTS-IN-ACTION.md).

<details>
<summary>Trust Card details</summary>

```bash
assay trust-card generate demo/fixtures/bundle.tar.gz --out-dir ./trust-out
# -> trust-out/trustcard.json , trust-out/trustcard.md , trust-out/trustcard.html
```

The Trust Card is a deterministic render of the same claim rows plus frozen non-goals; `trustcard.json` is canonical, while Markdown and static HTML are reviewer projections. Contract versions, pack floors, and release checklist: [MIGRATION — Trust Compiler 3.2](docs/architecture/MIGRATION-TRUST-COMPILER-3.2.md), [receipt-family matrix](docs/reference/receipt-family-matrix.json). Release history belongs in [CHANGELOG.md](CHANGELOG.md).

</details>

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

<details>
<summary>Other import paths and protocol adapters</summary>

### OpenTelemetry in, canonical evidence out

Assay ingests OpenTelemetry JSONL, builds replayable traces, and exports **canonical evidence** — OTel is a bridge, not the sole semantic authority.

```bash
assay trace ingest-otel \
  --input otel-export.jsonl \
  --db .eval/eval.db \
  --out-trace traces/otel.v2.jsonl
```

See [OpenTelemetry & Langfuse](docs/guides/otel-langfuse.md).

### Protocol adapters

Assay ships adapters that map protocol events into **canonical evidence**:

| Protocol | Adapter | What it maps |
|----------|---------|--------------|
| **ACP** (OpenAI/Stripe) | `assay-adapter-acp` | Checkout events, payment intents, tool calls |
| **A2A** (Google) | `assay-adapter-a2a` | Agent capabilities, task delegation, artifacts |
| **UCP** (Google/Shopify) | `assay-adapter-ucp` | Discover/buy/post-purchase state transitions |

Adapter crates are workspace / binary-driven, not published as separate `crates.io` packages.

</details>

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

## Why Assay

| | |
|---|---|
| **Canonical evidence** | Assay’s evidence model is the stable contract; OTel and adapters map into it. |
| **Deterministic** | Same input, same decision — not probabilistic. |
| **Portable artifacts** | Bundles, Trust Basis, Trust Card, SARIF — for CI, review, audit. |
| **Bounded claims** | Explicit about what is **verified** vs **visible** vs **absent** — no score-first UX. |
| **MCP-native wedge** | `assay mcp wrap` is the fast path; `assay mcp discover`, `assay mcp kill`, and `assay mcp tool` keep the runtime surface grouped. Adapters extend the same engine. |
| **Offline-first** | No backend required for core enforcement and bundle verification. |

<details>
<summary>Measured latency</summary>

On the M1 Pro/macOS fragmented-IPI harness, protected tool-decision path:

- Main protection run: `0.771ms` p50 / `1.913ms` p95
- Fast-path scenario: `0.345ms` p50 / `1.145ms` p95

These are tool-decision timings, not end-to-end model latency. (See [Research & experiments](#research-mappings-experiments) for methodology context.)

</details>

## Learn More

- [Promptfoo JSONL to Evidence Receipts](docs/use-cases/evidence-receipts-from-promptfoo-jsonl.md) — smallest adoption path for existing eval artifacts
- [OpenFeature EvaluationDetails to CI Review Artifact](docs/use-cases/openfeature-evaluationdetails-to-ci-review-artifact.md) — runtime decision receipt path
- [CycloneDX ML-BOM Model to Inventory Receipt](docs/use-cases/cyclonedx-mlbom-model-to-inventory-receipt.md) — model inventory/provenance receipt path
- [MCP Quickstart](examples/mcp-quickstart/) — filesystem server walkthrough
- [Policy Files](docs/reference/config/policies.md) — YAML schema for `assay mcp wrap`
- [OpenTelemetry & Langfuse](docs/guides/otel-langfuse.md) — traces → replay and evidence
- [CI Guide](docs/guides/github-action.md) — GitHub Action
- [Evidence Store](docs/guides/evidence-store-aws-s3.md) — S3, B2, MinIO
- [ADR-033: Trust compiler positioning](docs/architecture/ADR-033-OTel-Trust-Compiler-Positioning.md)
- [RFC-005: Trust compiler MVP & Trust Card](docs/architecture/RFC-005-trust-compiler-mvp-2026q2.md)

## Internal: Assay-Runner

Assay-Runner is an internal measured-run subsystem used by Assay's delegated Linux/eBPF acceptance path. It is **not a standalone product**. As of Phase 2D, the runner candidate is split into extraction-ready Rust crates (`assay-runner-schema`, `assay-runner-core`, `assay-runner-linux`) — all `publish = false` — plus the `runner-fixtures/` package tree (Node fixture marked `"private": true`; Python fixture has no distribution surface). Everything stays inside this repository.

- [Assay-Runner reference index](docs/reference/runner/index.md) — internal contracts, boundary map, slice history
- [Measured-run proof-bundle walkthrough](docs/reference/runner/examples/measured-run-proof-bundle.md) — read-only walkthrough for maintainers evaluating standalone use cases
- [Phase 2D consolidation audit](docs/reference/runner/phase-2d-consolidation-audit.md) — current burn-in criteria; the extraction question is closed until the criteria are observed and at least one concrete external use case appears

No release commitment. No timeline. No external demand has been measured.

## Research, mappings & experiments

**Bounded context:** numbers below support **mapping and experiments**, not a product “security score.”

- [OWASP MCP Top 10 Mapping](docs/security/OWASP-MCP-TOP10-MAPPING.md) — how Assay relates to each risk category (coverage is **not** a scalar guarantee).
- Third-party survey: popular MCP servers often show weak defaults — Assay adds policy + evidence; see discussion in the mapping doc.
- [Security experiments](docs/architecture/SYNTHESIS-TRUST-CHAIN-TRIFECTA-2026q2.md) — attack vectors and harness notes (methodology matters more than headline counts).
- [MCP tool evidence-binding quickstart](docs/experiments/mcp-tool-evidence-binding-harness-2026-05/QUICKSTART.md) —
  synthetic description→call→effect binding with bounded claims.
  Experiment-scoped; **not** a poisoning detector, and distinct from
  the supported [MCP policy quickstart](examples/mcp-quickstart/) above.

## Contributing

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

See [CONTRIBUTING.md](CONTRIBUTING.md). **Discussions:** [GitHub Discussions](https://github.com/Rul1an/assay/discussions) — seed topics for pinned threads live in [docs/community/DISCUSSIONS.md](docs/community/DISCUSSIONS.md).

## License

[MIT](LICENSE)
