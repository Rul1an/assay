# assay init

Initialize an Assay project.

---

## Synopsis

```bash
assay init [OPTIONS]
```

## Description

Scans your directory for known project types (MCP, Python, Node.js) and generates a security policy and config. Optionally generates CI scaffolding and `.gitignore`.

With `--from-trace`, generates a policy directly from recorded agent behavior instead of using a starter preset.

The `default` and `dev` presets allow all tool names except their explicit deny
list. Only `read_file` has an argument schema. The `hardened` preset uses an
explicit tool allowlist.

### Updating a starter policy

`assay init` keeps an existing `policy.yaml`. To update a project after upgrading
the CLI, run `assay init --preset <your-preset>` in an empty directory, compare the
new `policy.yaml` with your project's copy and merge the path rules you need.

The preset schemas check literal POSIX path strings for `read_file` and, in
`hardened`, `list_directory` and `search_files`. They require a configured root followed by a slash
and reject dot and parent components, doubled separators, backslashes and ASCII
control characters. Ordinary dotfiles and spaces are accepted. The server that
opens a file must enforce the filesystem boundary after resolving the path,
including symbolic links. Other tools need their own argument schemas.

## Options

| Option | Description |
|--------|-------------|
| `--config <FILE>` | Config filename. Default: `eval.yaml`. |
| `--ci [PROVIDER]` | Generate CI scaffolding. `github` (default) or `gitlab`. |
| `--gitignore` | Generate `.gitignore` for artifacts/db. |
| `--preset <PRESET>` | Starter preset: `default`, `hardened`, `dev`. Default: `default`. |
| `--list-presets` | List available presets and exit. |
| `--from-trace <FILE>` | Generate policy from an existing trace file (JSONL). |
| `--heuristics` | Enable entropy/risk analysis when generating from trace. Requires `--from-trace`. |
| `--hello-trace` | Generate a runnable hello trace and smoke suite scaffold. |

## Examples

### Basic setup
```bash
assay init
```

Creates `eval.yaml` with a canonical v1 scaffold (`configVersion: 1`) and a starter smoke test template you can replace with real prompts/expectations.

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
assay init --preset hardened --ci --gitignore

# Backward-compatible alias (deprecated)
assay init --pack hardened --ci --gitignore
```
