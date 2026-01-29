<h1 align="center">
  <br>
  <img src="assets/logo.svg" alt="Assay Logo" width="200">
  <br>
  Assay
  <br>
</h1>

<p class="subtitle">Policy-as-Code for AI Agents</p>

Assay is a **Policy-as-Code** engine for the Model Context Protocol (MCP). Deterministic testing, verifiable evidence bundles, and runtime enforcement.

---

## Install

```bash
curl -fsSL https://getassay.dev/install.sh | sh
```

## Core Capabilities

<div class="grid cards" markdown>

-   :material-shield-check:{ .lg .middle } __Policy Enforcement__

    ---

    Validate tool calls against JSON Schema constraints, sequence rules, and allowlists. No LLM calls in CI.

    [:octicons-arrow-right-24: Policy Reference](reference/policies.md)

-   :material-package-variant-closed:{ .lg .middle } __Evidence Bundles__

    ---

    Tamper-evident audit trails with content-addressed IDs. CloudEvents v1.0 format. SARIF output for GitHub Security.

    [:octicons-arrow-right-24: Evidence Guide](concepts/traces.md)

-   :material-clipboard-check:{ .lg .middle } __Compliance Packs__

    ---

    Built-in rule packs for EU AI Act, SOC 2, and custom policies. Article-referenced findings for auditors.

    [:octicons-arrow-right-24: Pack Engine](architecture/SPEC-Pack-Engine-v1.md)

-   :material-key:{ .lg .middle } __Tool Signing__

    ---

    Ed25519 signatures for tool definitions. DSSE envelope format. Trust policies for supply chain security.

    [:octicons-arrow-right-24: Signing Spec](architecture/SPEC-Tool-Signing-v1.md)

</div>

## Quick Start

### 1. Capture Traces

```bash
assay import --format mcp-inspector session.json --out trace.jsonl
```

### 2. Validate

```bash
assay validate --trace-file trace.jsonl --format sarif
```

### 3. Export Evidence

```bash
assay evidence export --out bundle.tar.gz
assay evidence verify bundle.tar.gz
```

### 4. Lint with Compliance Pack

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
- uses: Rul1an/assay/assay-action@v2
```

Zero-config. Discovers evidence bundles, verifies integrity, uploads SARIF to GitHub Security.

[:octicons-arrow-right-24: GitHub Action Guide](guides/github-action.md)

## Runtime Enforcement (Linux)

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
| EU AI Act Article 12 | Compliance pack mapping |

## Next Steps

- [**Getting Started**](getting-started/index.md)
- [**Python SDK**](getting-started/python-quickstart.md)
- [**CLI Reference**](reference/cli/index.md)
- [**Architecture**](architecture/index.md)
