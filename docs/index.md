<h1 align="center">
  <br>
  <img src="assets/logo.svg" alt="Assay Logo" width="200">
  <br>
  Assay
  <br>
</h1>

<p class="subtitle">CI-native evidence compiler for agent governance</p>

Assay compiles **agent runtime signals** and selected external outcomes into
**verifiable evidence** and bounded **Trust Basis claims**. MCP policy
enforcement is the wedge: Assay can sit between an agent and its tools, make
deterministic allow/deny decisions, and preserve the evidence chain for CI,
security review, and audit without a hosted backend.

---

## Install

```bash
curl -fsSL https://getassay.dev/install.sh | sh
```

## Core Capabilities

<div class="grid cards" markdown>

-   :material-shield-check:{ .lg .middle } __Protocol Policy Enforcement__

    ---

    Validate MCP tool calls against JSON Schema constraints, sequence rules, and allowlists. No LLM calls in CI.

    [:octicons-arrow-right-24: Policy Reference](reference/config/policies.md)

-   :material-package-variant-closed:{ .lg .middle } __Evidence Bundles__

    ---

    Tamper-evident audit trails with content-addressed IDs. Verify bundles offline and keep canonical evidence separate from projections.

    [:octicons-arrow-right-24: Evidence Guide](concepts/traces.md)

-   :material-clipboard-check:{ .lg .middle } __Trust Basis & Receipts__

    ---

    Compile verified bundles into Trust Basis claims and import bounded external receipt families for eval outcomes, runtime decisions, and model inventory.

    [:octicons-arrow-right-24: Receipt Matrix](reference/receipt-family-matrix.json)

-   :material-key:{ .lg .middle } __Tool Signing__

    ---

    Ed25519 signatures for tool definitions. DSSE envelope format. Trust policies for supply chain security.

    [:octicons-arrow-right-24: Signing Spec](architecture/SPEC-Tool-Signing-v1.md)

</div>

## Quick Start

### 1. Capture Traces

```bash
assay import --format inspector session.json --out-trace trace.jsonl
```

### 2. Validate

```bash
assay validate --trace-file trace.jsonl --format sarif
```

### 3. Export Evidence

```bash
assay profile init --output assay-profile.yaml --name quickstart
assay evidence export --profile assay-profile.yaml --out bundle.tar.gz
assay evidence verify bundle.tar.gz
```

### 4. Generate Trust Artifacts

```bash
assay trust-basis generate bundle.tar.gz --out trust-basis.json
assay trustcard generate bundle.tar.gz --out-dir trustcard
```

`trustcard.json` is the canonical Trust Card artifact. `trustcard.md` and
`trustcard.html` are deterministic reviewer projections of the same claim rows
and frozen non-goals.

### 5. Optional: Lint with a Pack

```bash
assay evidence lint --pack eu-ai-act-baseline bundle.tar.gz
```

| Result | Exit Code | Output |
|--------|-----------|--------|
| Pass | `0` | Summary |
| Fail | `1` | SARIF with findings |
| Error | `2` | Config/Schema validation |

## GitHub Action

```yaml
- uses: Rul1an/assay-action@v2
```

Zero-config. Discovers evidence bundles, verifies integrity, uploads SARIF to GitHub Security.

[:octicons-arrow-right-24: GitHub Action Guide](guides/github-action.md)

## Defense in Depth: Runtime Enforcement (Linux, Optional)

Optional kernel-level hardening for Linux deployments.

```bash
# Landlock sandbox (rootless)
assay sandbox --policy policy.yaml -- python agent.py

# eBPF/LSM kernel-level enforcement
sudo assay monitor --policy policy.yaml --pid <agent-pid>
```

## Standards Alignment

| Standard | Integration |
|----------|-------------|
| CloudEvents v1.0 | Evidence envelope format |
| W3C Trace Context | `traceparent` correlation |
| SARIF 2.1.0 | GitHub Code Scanning |
| EU AI Act Article 12 | Optional pack mapping |

## Next Steps

- [**Getting Started**](getting-started/index.md)
- [**Scope & Boundaries**](concepts/scope.md)
- [**Operator Proof Flow**](guides/operator-proof-flow.md)
- [**Python SDK**](getting-started/python-quickstart.md)
- [**OpenTelemetry & Langfuse**](guides/otel-langfuse.md)
- [**CLI Reference**](reference/cli/index.md)
- [**Architecture**](architecture/index.md)
