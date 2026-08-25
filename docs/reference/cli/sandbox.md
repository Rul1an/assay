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
- **Optional TCP destination-port allowlisting** via `--enforce --enforce-net`
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
| `--enforce` | Require active Landlock filesystem enforcement |
| `--enforce-net` | Enforce an explicit TCP destination-port allowlist; requires `--enforce` |
| `--enforcement-health <path>` | Write `assay.enforcement_health.v1` (Landlock TCP-connect) to this path; requires `--enforce-net` |
| `--dry-run` | Audit without active blocking; conflicts with `--enforce` |

### Environment Control

| Option | Description |
|--------|-------------|
| `--env-allow <VAR>` | Allow specific env var(s) through the scrub filter |
| `--env-strict` | Strict mode: only safe base vars + explicit allows |
| `--env-passthrough` | ⚠️ DANGER: Pass all env vars (disables scrubbing) |
| `--env-strip-exec` | Strip execution-related env vars (e.g. `PATH`, loader/debug vars) after filtering |
| `--env-safe-path` | Reset `PATH` inside the sandbox to a minimal, known-safe search path |

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

### Profiling Output

When you use `--profile`, Assay now writes three related artifacts:

- the existing policy suggestion at the path you requested
- the human-readable report (`*.report.md` by default)
- a machine-readable evidence profile sidecar (`*.evidence.yaml` or `*.evidence.json`)

That evidence profile sidecar is the canonical input for `assay evidence export`
when you want bundle evidence from a sandboxed run, including
`assay.sandbox.degraded` for supported weaker-than-requested fallback paths.

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

### Implicit baseline

On a supported Linux host, the sandbox gives the working directory
read-and-execute access and its scoped temporary directory full access before
adding explicit policy allows. A custom `fs.allow` grants the current
full-access Landlock permission set beneath the named path.

### Custom Policy

```yaml
# my-policy.yaml
api_version: "assay/v1"

fs:
  allow:
    - "${CWD}/data"
  deny:
    - "${HOME}/.ssh"
    - "${HOME}/.aws"

net:
  allow: []
  deny: []
```

If the file named by `--policy` does not parse, `assay sandbox` exits 2 with
`E_POLICY_LOAD_FAILED_UNENFORCEABLE` and runs nothing. It does not fall back to a built-in
pack, because that would run the workload under containment you did not choose.

```bash
assay sandbox --policy my-policy.yaml -- ./mcp-server
```

### Path Variables

| Variable | Expansion |
|----------|-----------|
| `${CWD}` | Current working directory |
| `${HOME}` | User home directory |
| `${USER}` | Current username |

The sandbox policy format does not implement `/**` globs or per-operation
read/write/execute fields. `${TMPDIR}` is not expanded; Assay's scoped
temporary directory is already part of the baseline.

---

## Landlock Limitations

Landlock is an **allow-only** LSM. It cannot enforce "deny X inside allowed Y".

### Conflict Example

```yaml
fs:
  allow:
    - "${HOME}"      # Allow all of home
  deny:
    - "${HOME}/.ssh" # Try to deny .ssh
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

When degraded execution continues and profiling is enabled, the evidence profile
sidecar can carry a typed `assay.sandbox.degraded` signal for the supported
fallback paths. Intentional audit/permissive runs and fail-closed aborts do not
emit that signal.

`--enforcement-health` requires `--enforce-net`. If execution degrades to audit
before Landlock is applied (unsupported backend, or a policy conflict that is
not `--fail-closed`), the mechanism artifact is not written and Assay names the
requested path on stderr. That diagnostic is not an `assay.enforcement_health.v1`
record.

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
| 2 | Policy cannot be enforced: a named `--policy` that is missing or does not parse, or a conflict under `--fail-closed` |

Codes 3 and 4 were previously documented for a missing policy file and invalid policy
syntax. No code path emitted them: `exit_codes.rs` reserves 3 for infrastructure failures
and 4 for would-block, and both policy cases are configuration errors, which is 2.

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
- [assay doctor](./doctor.md)
