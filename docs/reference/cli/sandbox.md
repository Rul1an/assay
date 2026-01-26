# assay sandbox

Run a command inside a hardened sandbox with Landlock enforcement.

---

## Synopsis

```bash
assay sandbox [OPTIONS] -- <COMMAND> [ARGS...]
```

---

## Description

The `assay sandbox` command executes an MCP server or any command inside a security sandbox. It provides:

- **Filesystem isolation** via Linux Landlock LSM
- **Network control** (audit or block)
- **Environment scrubbing** (credential leak prevention)
- **Scoped `/tmp`** (per-run isolation)

This is the recommended way to run untrusted MCP servers in CI/CD or development.

---

## Options

### Security

| Option | Description |
|--------|-------------|
| `--policy`, `-p` | Path to sandbox policy YAML (default: built-in minimal) |
| `--fail-closed` | Exit if policy cannot be fully enforced (no degradation) |

### Environment Control

| Option | Description |
|--------|-------------|
| `--env-allow <VAR>` | Allow specific env var(s) through the scrub filter |
| `--env-strict` | Strict mode: only safe base vars + explicit allows |
| `--env-passthrough` | ⚠️ DANGER: Pass all env vars (disables scrubbing) |

### Execution

| Option | Description |
|--------|-------------|
| `--workdir`, `-w` | Working directory for command (default: current) |
| `--timeout` | Kill command after N seconds |

### Output

| Option | Description |
|--------|-------------|
| `--verbose`, `-v` | Show detailed sandbox setup |
| `--quiet`, `-q` | Suppress banner output |

---

## Environment Scrubbing

By default, `assay sandbox` **scrubs sensitive environment variables** to prevent credential leakage to untrusted processes.

### Default Behavior (Pattern-Based Scrub)

Variables matching these patterns are **removed**:

```
*_TOKEN, *_SECRET, *_KEY, *_PASSWORD, *_CREDENTIAL*
AWS_*, OPENAI_*, ANTHROPIC_*, GITHUB_*, GITLAB_*
DATABASE_URL, *_DATABASE_URL, *_CONNECTION_STRING
SSH_*, GPG_*, VAULT_*, KUBECONFIG
LD_PRELOAD, LD_LIBRARY_PATH, PYTHONPATH, NODE_OPTIONS
```

Variables matching these patterns **pass through**:

```
PATH, HOME, USER, SHELL, LANG, LC_*, TERM
TMPDIR, TMP, TEMP, XDG_*
RUST_LOG, RUST_BACKTRACE, CARGO_*
EDITOR, PAGER, CLICOLOR, NO_COLOR
```

### Strict Mode (`--env-strict`)

Only safe base variables pass through. Everything else is scrubbed:

```bash
# Only PATH, HOME, USER, SHELL, LANG, TERM, etc.
assay sandbox --env-strict -- ./mcp-server
```

### Explicit Allow (`--env-allow`)

Pass specific variables through the filter:

```bash
# Allow custom config var
assay sandbox --env-allow MY_CONFIG_PATH -- ./mcp-server

# Allow multiple vars
assay sandbox --env-allow VAR1 --env-allow VAR2 -- ./mcp-server
```

### Passthrough Mode (`--env-passthrough`)

⚠️ **DANGER**: Disables all scrubbing. Use only for debugging:

```bash
# NOT RECOMMENDED for untrusted code
assay sandbox --env-passthrough -- ./mcp-server
```

---

## Filesystem Policies

Sandbox policies control filesystem access using Landlock LSM.

### Built-in Policies

| Policy | Description |
|--------|-------------|
| `minimal` | Read-only CWD, write to scoped /tmp only |
| `development` | Read CWD, write to CWD + /tmp |
| `mcp-server` | Tailored for typical MCP server needs |

### Custom Policy

```yaml
# my-policy.yaml
version: "1.0"
name: "my-sandbox"

fs:
  allow:
    - path: "${CWD}/**"
      read: true
      write: false
    - path: "${TMPDIR}/**"
      read: true
      write: true
  deny:
    - path: "${HOME}/.ssh/**"
    - path: "${HOME}/.aws/**"

net:
  mode: audit  # audit | block | allow
```

```bash
assay sandbox --policy my-policy.yaml -- ./mcp-server
```

### Path Variables

| Variable | Expansion |
|----------|-----------|
| `${CWD}` | Current working directory |
| `${HOME}` | User home directory |
| `${TMPDIR}` | Scoped temp directory |
| `${USER}` | Current username |

---

## Landlock Limitations

Landlock is an **allow-only** LSM. It cannot enforce "deny X inside allowed Y".

### Conflict Example

```yaml
fs:
  allow:
    - path: "${HOME}/**"      # Allow all of home
  deny:
    - path: "${HOME}/.ssh/**" # Try to deny .ssh
```

**Problem**: Landlock cannot block `.ssh` because it's inside the allowed `${HOME}`.

### How Assay Handles This

1. **Detects the conflict** before enforcement
2. **Warns** and degrades to Audit mode (no containment)
3. **With `--fail-closed`**: Exits immediately with code 2

```bash
# Default: warns and continues
assay sandbox --policy conflict.yaml -- ./cmd
# WARN: Landlock cannot enforce deny inside allowed path
# INFO: Degrading to Audit mode

# Strict: fails on unenforceable policy
assay sandbox --fail-closed --policy conflict.yaml -- ./cmd
# ERROR: Policy cannot be fully enforced
# exit 2
```

---

## Scoped /tmp

Each sandbox run gets an isolated temporary directory:

```
/tmp/assay-<UID>-<PID>/
```

Features:
- **UID from kernel** (not spoofable `$USER`)
- **PID isolation** (no cross-run interference)
- **0700 permissions** (owner-only access)
- **Auto-cleanup** on exit

The following env vars are set to this path:
- `TMPDIR`
- `TMP`
- `TEMP`

---

## Examples

### Basic Usage

```bash
# Run MCP server in sandbox
assay sandbox -- npx @modelcontextprotocol/server-filesystem

# With custom working directory
assay sandbox --workdir /project -- ./mcp-server
```

### CI/CD Pipeline

```bash
# Strict security for untrusted code
assay sandbox \
  --policy policies/ci-locked.yaml \
  --env-strict \
  --fail-closed \
  --timeout 300 \
  -- ./untrusted-mcp-server
```

### Development

```bash
# Allow API key for testing (explicit opt-in)
assay sandbox \
  --env-allow OPENAI_API_KEY \
  -- ./my-agent

# Verbose output for debugging
assay sandbox --verbose -- ./mcp-server
```

### With Custom Policy

```bash
# Custom filesystem rules
assay sandbox --policy my-policy.yaml -- ./mcp-server
```

---

## Banner Output

```
Assay Sandbox v2.4
──────────────────
Backend: Landlock (Containment)
  FS:    contain
  Net:   audit (kernel < 6.7)
  Env:   scrubbed (42 passed, 7 removed)
Policy:  my-policy.yaml
Workdir: /home/user/project
Tmp:     /tmp/assay-1000-12345
──────────────────
```

### Degraded Mode

```
Assay Sandbox v2.4
──────────────────
Backend: Landlock (Audit)
  FS:    audit (degraded)
  Net:   audit
  Env:   scrubbed (42 passed, 7 removed)
⚠ Degradations: 1 (Landlock conflict → no containment)
──────────────────
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Command succeeded |
| 1 | Command failed (pass-through exit code) |
| 2 | Policy cannot be enforced (`--fail-closed`) |
| 3 | Policy file not found |
| 4 | Invalid policy syntax |

---

## Diagnostics

Use `assay doctor` to verify sandbox capabilities:

```bash
assay doctor

# Output:
# Sandbox Hardening:
#   Env Scrubbing:        ✓ (67 patterns)
#   Exec-Influence Scrub: ✓ (LD_PRELOAD, PYTHONPATH, ...)
#   Scoped /tmp:          ✓ (UID+PID, 0700)
#   Fork-safe pre_exec:   ✓
#   Deny Conflict Det:    ✓
#   Landlock:             ✓ ABI v4 (FS + Net)
```

---

## Security Considerations

### Threat Model

The sandbox protects against:

| Threat | Mitigation |
|--------|------------|
| Credential exfiltration | Env scrubbing (default-deny secrets) |
| Filesystem escape | Landlock containment |
| Execution hijacking | LD_PRELOAD/PYTHONPATH scrubbing |
| Cross-run interference | Scoped /tmp per process |
| Symlink attacks | Inode-based path resolution |

### What It Does NOT Protect Against

- Kernel exploits (root/CAP_SYS_ADMIN)
- Network exfiltration (unless `net: block` policy)
- Side-channel attacks
- Attacks within allowed filesystem scope

### Recommendations

1. Use `--env-strict` for untrusted code
2. Use `--fail-closed` in production CI
3. Keep allow paths minimal
4. Prefer explicit `--env-allow` over `--env-passthrough`

---

## See Also

- [Sandbox Security Guide](../../guides/sandbox-security.md)
- [Environment Filtering Reference](../sandbox-env.md)
- [Sandbox Policies Reference](../sandbox-policies.md)
- [assay doctor](doctor.md)
