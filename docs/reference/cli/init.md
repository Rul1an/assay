# assay init

Initialize an Assay project.

---

## Synopsis

```bash
assay init [OPTIONS]
```

## Description

Scans your directory for known project types (MCP, Python, Node.js) and generates a security policy and config. Optionally generates CI scaffolding and `.gitignore`.

With `--from-trace`, generates a policy directly from recorded agent behavior instead of using a preset pack.

## Options

| Option | Description |
|--------|-------------|
| `--config <FILE>` | Config filename. Default: `eval.yaml`. |
| `--ci [PROVIDER]` | Generate CI scaffolding. `github` (default) or `gitlab`. |
| `--gitignore` | Generate `.gitignore` for artifacts/db. |
| `--pack <PACK>` | Policy pack: `default`, `hardened`, `dev`. Default: `default`. |
| `--list-packs` | List available packs and exit. |
| `--from-trace <FILE>` | Generate policy from an existing trace file (JSONL). |
| `--heuristics` | Enable entropy/risk analysis when generating from trace. Requires `--from-trace`. |
| `--hello-trace` | Generate a runnable hello trace and smoke suite scaffold. |

## Examples

### Basic setup
```bash
assay init
```

### Fast first signal (hello trace)
Creates `eval.yaml`, `traces/hello.jsonl`, and `policy.yaml` (if missing).
When `--config` points to another directory, the hello trace is written relative to that config path.

```bash
assay init --hello-trace
assay validate --config eval.yaml --trace-file traces/hello.jsonl

# Config in a nested directory:
assay init --hello-trace --config nested/eval.yaml
assay validate --config nested/eval.yaml --trace-file nested/traces/hello.jsonl
```

### From existing trace
```bash
assay init --from-trace traces/agent.jsonl --heuristics
```

### With GitHub Actions CI
```bash
assay init --ci
```

### With GitLab CI
```bash
assay init --ci gitlab
```

### Hardened policy
```bash
assay init --pack hardened --ci --gitignore
```
