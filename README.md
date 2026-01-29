# Assay

[![Crates.io](https://img.shields.io/crates/v/assay-cli.svg)](https://crates.io/crates/assay-cli)
[![CI](https://github.com/Rul1an/assay/actions/workflows/ci.yml/badge.svg)](https://github.com/Rul1an/assay/actions/workflows/ci.yml)
[![License](https://img.shields.io/crates/l/assay-core.svg)](https://github.com/Rul1an/assay/blob/main/LICENSE)
[![Open Core](https://img.shields.io/badge/Open%20Core-ADR--016-blue)](docs/architecture/ADR-016-Pack-Taxonomy.md)

**Policy-as-Code for AI Agents.**
Deterministic testing, runtime enforcement, and verifiable evidence for the Model Context Protocol.

> **Open Core:** Engine + baseline packs are open source (MIT/Apache-2.0).
> Enterprise packs and managed workflows are commercial.
> See [ADR-016](docs/architecture/ADR-016-Pack-Taxonomy.md) for details.

## Install

```bash
curl -fsSL https://getassay.dev/install.sh | sh
```

Or via Cargo:
```bash
cargo install assay-cli
```

## Core Workflow

### 1. Record → Replay → Validate

Record agent behavior once, replay deterministically in CI. No LLM calls, no flakiness.

```bash
# Capture traces from your agent
assay import --format mcp-inspector session.json --out trace.jsonl

# Validate against policy (milliseconds, $0 cost)
assay validate --config assay.yaml --trace-file trace.jsonl

# CI gate with SARIF output
assay run --config assay.yaml --format sarif
```

### 2. Generate Policies from Behavior

```bash
# Single trace → policy
assay generate -i trace.jsonl --heuristics

# Multi-run profiling for stable policies
assay profile init --output profile.yaml --name my-app
assay profile update --profile profile.yaml -i trace.jsonl --run-id ci-123
assay generate --profile profile.yaml --min-stability 0.8
```

### 3. Evidence Bundles

Tamper-evident bundles with content-addressed IDs. CloudEvents v1.0 format.

```bash
# Export evidence
assay evidence export --profile profile.yaml --out bundle.tar.gz

# Verify integrity
assay evidence verify bundle.tar.gz

# Lint for security issues (SARIF output)
assay evidence lint bundle.tar.gz --format sarif

# Lint with compliance pack
assay evidence lint --pack eu-ai-act-baseline bundle.tar.gz

# Compare runs
assay evidence diff baseline.tar.gz current.tar.gz
```

### 4. Compliance Packs

Built-in rule packs for regulatory compliance. Article-referenced, auditor-friendly.

```bash
# EU AI Act Article 12 (logging requirements)
assay evidence lint --pack eu-ai-act-baseline bundle.tar.gz

# Multiple packs
assay evidence lint --pack eu-ai-act-baseline,soc2-baseline bundle.tar.gz

# Custom pack
assay evidence lint --pack ./my-org-rules.yaml bundle.tar.gz
```

SARIF output includes article references for audit trails.

### 5. Pack Registry (Secure, Reproducible Pack Fetching)

Assay resolves `--pack` references in a deterministic order:
1. **Local** (`./custom.yaml`)
2. **Bundled** (`packs/open/<name>`)
3. **Registry** (`name@version` or pinned `name@version#sha256:...`)
4. **BYOS** (`s3://`, `gs://`, `az://`)

All remote packs are verified before use:
- **Canonical digest**: strict YAML subset → JSON → JCS (RFC 8785) → SHA-256
- **Authenticity**: Ed25519 + DSSE signature verification for commercial packs
- **Sidecar signatures**: `GET /packs/{name}/{version}.sig` (avoids header size limits)

Trust model is **no-TOFU**:
- CLI ships with pinned root key IDs
- Registry publishes a DSSE-signed keys manifest (`GET /keys`)
- Pack signatures must chain to manifest keys (revocation/expiry enforced)

For reproducible CI, `assay.packs.lock` (v2) pins name/version/digest/signature metadata. Lockfile mismatches are hard errors.

See [SPEC-Pack-Registry-v1](docs/architecture/SPEC-Pack-Registry-v1.md) for the full protocol specification.

### 6. Tool Signing

Cryptographic signatures for tool definitions. Ed25519 + DSSE.

```bash
# Generate keypair
assay tool keygen --out ~/.assay/keys/

# Sign tool definition
assay tool sign tool.json --key priv.pem --out signed.json

# Verify signature
assay tool verify signed.json --pubkey pub.pem
```

### 7. BYOS (Bring Your Own Storage)

Push evidence to your own S3-compatible storage. No vendor lock-in.

```bash
# Push bundle
assay evidence push bundle.tar.gz --store s3://my-bucket/evidence

# Pull by ID
assay evidence pull --bundle-id sha256:abc... --store s3://my-bucket/evidence

# List bundles
assay evidence list --store s3://my-bucket/evidence
```

Supports: AWS S3, Backblaze B2, Cloudflare R2, MinIO, Azure Blob, GCS.

## Runtime Enforcement

### MCP Server Proxy

```bash
# Start policy enforcement proxy
assay mcp-server --policy policy.yaml
```

### Kernel-Level Sandbox (Linux)

```bash
# Landlock isolation (rootless)
assay sandbox --policy policy.yaml -- python agent.py

# eBPF/LSM enforcement (requires capabilities)
sudo assay monitor --policy policy.yaml --pid <agent-pid>
```

## GitHub Action

```yaml
- uses: Rul1an/assay/assay-action@v2
```

Zero-config evidence verification. Native GitHub Security tab integration.

**v2.1 features:**
- Compliance packs (`pack: eu-ai-act-baseline`)
- BYOS push with OIDC (`store: s3://bucket/evidence`)
- Artifact attestation (`attest: true`)
- Coverage badges

```yaml
# Full example
- uses: Rul1an/assay/assay-action@v2
  with:
    pack: eu-ai-act-baseline
    store: s3://my-bucket/evidence
    store_role: arn:aws:iam::123456789:role/AssayRole
    attest: true
```

See [GitHub Marketplace](https://github.com/marketplace/actions/assay-ai-agent-security) | [Guide](docs/guides/github-action.md).

## Configuration

`assay.yaml`:
```yaml
version: "2.0"
name: "mcp-default-gate"

allow: ["*"]

deny:
  - "exec*"
  - "shell*"

constraints:
  - tool: "read_file"
    params:
      path:
        matches: "^/app/.*|^/data/.*"
```

## Python SDK

```bash
pip install assay
```

```python
from assay import AssayClient, validate

# Record traces
client = AssayClient("traces.jsonl")
client.record_trace(tool_call)

# Validate
result = validate("policy.yaml", traces)
assert result["passed"]
```

Pytest plugin for automatic trace capture:
```python
@pytest.mark.assay(trace_file="test_traces.jsonl")
def test_agent():
    pass
```

## Documentation

- [Getting Started](https://getassay.dev/getting-started/)
- [Policy Reference](https://getassay.dev/reference/policies/)
- [Evidence Bundles](https://getassay.dev/concepts/traces/)
- [GitHub Action](https://getassay.dev/guides/github-action/)
- [Python SDK](https://getassay.dev/python-sdk/)

## Contributing

```bash
cargo test --workspace
```

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](LICENSE)
