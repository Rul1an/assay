# Quick Start

Run your first Assay validation in 60 seconds.

## Prerequisites

- Assay installed ([installation guide](installation.md))

## 1. Initialize

```bash
assay init --hello-trace
```

Generates a runnable smoke setup:
- `eval.yaml` with a minimal `hello_smoke` suite
- `traces/hello.jsonl` with a deterministic trace fixture
- `policy.yaml` with the default policy pack (created if not already present)

If you pass `--config <path>`, the hello trace is written relative to that config directory.

The hello trace is demo-only and non-sensitive. Treat real traces as potentially sensitive data and apply your redaction/recording guidelines.

## 2. Validate

```bash
assay validate --config eval.yaml --trace-file traces/hello.jsonl
```

## 3. Capture Your Own Traces

After the hello smoke passes, import from MCP Inspector or create your own trace file:

```bash
# From MCP Inspector
assay import --format inspector session.json --out-trace trace.jsonl

# Or create manually
echo '{"tool": "read_file", "args": {"path": "/etc/passwd"}}' > trace.jsonl
```

## 4. Validate Your Own Trace

```bash
assay validate --trace-file trace.jsonl
```

Output:
```
✖ Validation failed (1 error)

[E_POLICY_VIOLATION] read_file
  Path '/etc/passwd' matches blocked pattern
```

## 5. Export Evidence

Create a verifiable evidence bundle:

```bash
assay profile init --output assay-profile.yaml --name quickstart
assay evidence export --profile assay-profile.yaml --out bundle.tar.gz
assay evidence verify bundle.tar.gz
```

Bundles are content-addressed (SHA-256). Tamper-evident.

## 6. Lint for Issues

```bash
# Basic lint
assay evidence lint bundle.tar.gz --format sarif

# With compliance pack
assay evidence lint --pack eu-ai-act-baseline bundle.tar.gz
```

SARIF output integrates with GitHub Code Scanning.

## 7. CI Integration

```bash
assay init --ci
```

Creates `.github/workflows/assay.yml`. PRs that violate policy are blocked.

Or use the GitHub Action directly:

```yaml
- uses: Rul1an/assay/assay-action@v2
```

## 8. Runtime Enforcement (Linux)

Kernel-level blocking:

```bash
# Landlock sandbox (rootless)
assay sandbox --policy policy.yaml -- python agent.py

# eBPF/LSM (requires capabilities)
sudo assay monitor --policy policy.yaml --pid <pid>
```

Requires Linux 5.8+ with BPF LSM support.

---

## Core Commands

| Command | Purpose |
|---------|---------|
| `assay validate` | Check traces against policy |
| `assay run` | Execute with policy enforcement |
| `assay evidence export` | Create evidence bundle |
| `assay evidence verify` | Verify bundle integrity |
| `assay evidence lint` | Security/compliance findings |
| `assay evidence diff` | Compare bundles |

## Next Steps

<div class="grid cards" markdown>

-   :material-file-document:{ .lg .middle } __Write a Policy__

    ---

    Custom constraints and sequences.

    [:octicons-arrow-right-24: Policy Reference](../reference/policies.md)

-   :material-github:{ .lg .middle } __GitHub Action__

    ---

    Automated verification in CI.

    [:octicons-arrow-right-24: Action Guide](../guides/github-action.md)

-   :material-package-variant-closed:{ .lg .middle } __Evidence Bundles__

    ---

    Audit trails and compliance.

    [:octicons-arrow-right-24: Evidence Guide](../concepts/traces.md)

-   :material-clipboard-check:{ .lg .middle } __Compliance Packs__

    ---

    EU AI Act, SOC 2 rule sets.

    [:octicons-arrow-right-24: Pack Engine](../architecture/SPEC-Pack-Engine-v1.md)

</div>

---

## Troubleshooting

### "No trace file found"

```bash
assay import --format inspector session.json --out-trace trace.jsonl
```

### "Config version mismatch"

```bash
assay migrate --config eval.yaml
```

### "Unknown tool in policy"

Tool names must match exactly. List tools in a trace:

```bash
awk -F'"' '/"tool"/ {print $4}' trace.jsonl | sort -u

# Then verify coverage against config
assay trace verify --trace trace.jsonl --config eval.yaml
```
