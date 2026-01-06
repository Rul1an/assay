# assay init

Initialize an Assay project with secure defaults.

---

## Synopsis

```bash
assay init [OPTIONS]
```

## Description

The `init` command is the fastest way to secure your agent. It scans your directory for known project types (MCP, Python, Node.js) and generates an opinionated security policy and CI configuration.

It generates:
- `assay.yaml`: The Policy-as-Code definition.
- `policy.yaml`: Baseline security rules (blocking Exec/Shell/Python by default).
- `.github/workflows/assay.yml`: (Optional) A complete CI/CD pipeline.

## Options

| Option | Description |
|--------|-------------|
| `--ci [PROVIDER]` | Generate CI scaffolding. Default: `github`. |
| `--config <FILE>` | Config filename. Default: `assay.yaml`. |
| `--gitignore` | Add `.assay` and trace files to `.gitignore`. |

## Examples

### Basic Setup
Detect project type and generate config.
```bash
assay init
```

### GitHub Actions
Generate a complete CI workflow for Pull Request gating.
```bash
assay init --ci
```
