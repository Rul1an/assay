# Assay GitHub Action

**Runtime Security & Policy Enforcement for AI Agents.**

Automatically validate agent traces against your security policy in CI/CD.

## Usage

```yaml
- uses: Rul1an/assay/assay-action@v1
  with:
    # Security policy file (YAML)
    policy: policies/agent.yaml

    # Path to trace files (or directory)
    traces: traces/

    # Fail if coverage drops below 80%
    min-coverage: 80
```

## Inputs

| Input | Description | Required | Default |
| :--- | :--- | :---: | :---: |
| `policy` | Path to `policy.yaml` | Yes | - |
| `traces` | Path to JSONL trace file(s) | Yes | - |
| `min-coverage` | Fail if coverage % < threshold | No | `80` |
| `fail-on-high-risk` | Fail if high-risk tools are uncovered | No | `true` |
| `format` | Output format (`github`, `json`, `markdown`) | No | `github` |
| `version` | Assay binary version to install | No | `latest` |

## How it works

1.  Installs the ultra-fast `assay` Rust binary.
2.  Runs `assay coverage` against your traces.
3.  Reports violations and coverage gaps directly in GitHub Actions (annotations).
4.  Fails the build if security policies are violated or coverage is too low.

## License

MIT
