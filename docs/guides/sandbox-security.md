# Sandbox Security Guide

This guide explains how to secure MCP servers and AI agents using Assay's sandbox.

---

## Why Sandbox?

MCP servers and AI agents execute code that may:

- **Exfiltrate credentials** via environment variables
- **Access sensitive files** outside their intended scope
- **Make unauthorized network connections**
- **Interfere with other processes** via shared /tmp

Assay's sandbox mitigates these risks using Linux Landlock LSM, environment scrubbing, and process isolation.

---

## Quick Start

```bash
# Run an MCP server in a secure sandbox
assay sandbox -- npx @modelcontextprotocol/server-filesystem

# With maximum security
assay sandbox --env-strict --fail-closed -- ./untrusted-server
```

---

## Security Layers

Assay implements defense-in-depth with multiple security layers:

```
┌─────────────────────────────────────────────────┐
│  Layer 1: Environment Scrubbing                 │
│  ─────────────────────────────────────────────  │
│  Removes secrets before process starts          │
│  AWS_*, GITHUB_TOKEN, *_SECRET, etc.            │
└─────────────────────────────────────────────────┘
                       ↓
┌─────────────────────────────────────────────────┐
│  Layer 2: Execution Influence Scrubbing         │
│  ─────────────────────────────────────────────  │
│  Removes behavior-modifying vars                │
│  LD_PRELOAD, PYTHONPATH, NODE_OPTIONS           │
└─────────────────────────────────────────────────┘
                       ↓
┌─────────────────────────────────────────────────┐
│  Layer 3: Filesystem Containment (Landlock)     │
│  ─────────────────────────────────────────────  │
│  Kernel-enforced path restrictions              │
│  Process cannot escape allowed paths            │
└─────────────────────────────────────────────────┘
                       ↓
┌─────────────────────────────────────────────────┐
│  Layer 4: Scoped /tmp Isolation                 │
│  ─────────────────────────────────────────────  │
│  Per-run temp directory with UID+PID            │
│  0700 permissions, no cross-run access          │
└─────────────────────────────────────────────────┘
```

---

## Environment Scrubbing

### The Problem

When you run `npx some-mcp-server`, it inherits your shell's environment:

```bash
env | grep -i secret
# AWS_SECRET_ACCESS_KEY=AKIAXXXXXXXXXXXXXXXX
# GITHUB_TOKEN=ghp_xxxxxxxxxxxxxxxxxxxx
# OPENAI_API_KEY=sk-xxxxxxxxxxxxxxxx
```

A malicious or compromised MCP server could exfiltrate these credentials.

### The Solution

Assay scrubs sensitive variables **before** the process starts:

```bash
# Without Assay:
npx mcp-server  # Has access to all your secrets 😱

# With Assay:
assay sandbox -- npx mcp-server  # Secrets removed ✓
```

### Scrub Modes

| Mode | CLI Flag | Behavior |
|------|----------|----------|
| **Pattern Scrub** | (default) | Remove known secret patterns |
| **Strict** | `--env-strict` | Only allow safe base vars |
| **Passthrough** | `--env-passthrough` | Allow everything (danger!) |

### Pattern Scrub (Default)

Removes variables matching dangerous patterns while allowing common dev tools:

**Removed:**
```
AWS_SECRET_ACCESS_KEY, GITHUB_TOKEN, OPENAI_API_KEY,
DATABASE_URL, SSH_AUTH_SOCK, VAULT_TOKEN, ...
```

**Allowed:**
```
PATH, HOME, USER, SHELL, LANG, TERM, EDITOR,
RUST_LOG, CARGO_HOME, XDG_CONFIG_HOME, ...
```

### Strict Mode

For maximum security with untrusted code:

```bash
assay sandbox --env-strict -- ./untrusted-server
```

Only these variables pass through:
- `PATH`, `HOME`, `USER`, `SHELL`
- `LANG`, `LC_*`, `TERM`
- `TMPDIR`, `TMP`, `TEMP`

Everything else requires explicit `--env-allow`:

```bash
assay sandbox --env-strict --env-allow MY_CONFIG -- ./server
```

### Explicit Allow

When you need specific variables:

```bash
# Allow one var
assay sandbox --env-allow OPENAI_API_KEY -- ./my-agent

# Allow multiple
assay sandbox \
  --env-allow OPENAI_API_KEY \
  --env-allow ANTHROPIC_API_KEY \
  -- ./my-agent
```

---

## Execution Influence Protection

### The Problem

Variables like `LD_PRELOAD` and `PYTHONPATH` can hijack execution:

```bash
# Attacker sets this in a shared environment:
export LD_PRELOAD=/tmp/evil.so

# Your innocent command now loads malicious code:
./my-server  # Loads evil.so first!
```

### The Solution

Assay scrubs execution-influence variables by default:

| Variable | Risk |
|----------|------|
| `LD_PRELOAD` | Inject shared library into process |
| `LD_LIBRARY_PATH` | Redirect library loading |
| `PYTHONPATH` | Inject Python modules |
| `NODE_OPTIONS` | Inject Node.js flags/requires |
| `RUBYOPT` | Inject Ruby options |
| `JAVA_TOOL_OPTIONS` | Inject JVM options |

These are scrubbed even in pattern mode (not just strict mode).

---

## Filesystem Containment

### How Landlock Works

Landlock is a Linux Security Module that restricts filesystem access:

```
Process: ./mcp-server

Allowed paths:
  /home/user/project/**  (read)
  /tmp/assay-1000-123/** (read+write)

Blocked paths:
  /home/user/.ssh/**     ← DENIED
  /etc/shadow            ← DENIED
  /                      ← DENIED
```

### Policy Example

```yaml
# my-policy.yaml
version: "1.0"
name: "restricted-mcp"

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
    - path: "${HOME}/.gnupg/**"
```

```bash
assay sandbox --policy my-policy.yaml -- ./mcp-server
```

### Landlock Limitations

Landlock is **allow-only**. It cannot enforce "deny X inside allowed Y":

```yaml
# ❌ This CANNOT be enforced:
fs:
  allow:
    - path: "${HOME}/**"      # Allow all of home
  deny:
    - path: "${HOME}/.ssh/**" # Deny .ssh (INSIDE allowed path)
```

**Assay detects this conflict** and:
- **Default**: Warns and degrades to Audit mode (no containment)
- **`--fail-closed`**: Exits with code 2

### Best Practice: Minimal Allow Paths

```yaml
# ✅ Good: Specific paths
fs:
  allow:
    - path: "${CWD}/src/**"
    - path: "${CWD}/data/**"
    - path: "${TMPDIR}/**"

# ❌ Bad: Overly broad
fs:
  allow:
    - path: "${HOME}/**"  # Too permissive!
```

---

## Scoped /tmp Isolation

### The Problem

Shared `/tmp` allows cross-process attacks:

```bash
# Process A writes:
echo "malicious" > /tmp/config

# Process B reads (expecting legitimate config):
cat /tmp/config  # Gets malicious content!
```

### The Solution

Assay creates a unique temp directory per run:

```
/tmp/assay-<UID>-<PID>/
       │      │
       │      └── Process ID (per-run unique)
       └── Kernel UID (not spoofable $USER)
```

Features:
- **0700 permissions** (owner-only)
- **Kernel UID** (cannot be spoofed)
- **PID scoping** (no interference between runs)
- **Auto-cleanup** on exit

The sandbox sets `TMPDIR`, `TMP`, and `TEMP` to this path.

---

## CI/CD Integration

### GitHub Actions

```yaml
name: Agent Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Assay
        run: cargo install assay-cli

      - name: Run MCP Server (Sandboxed)
        run: |
          assay sandbox \
            --env-strict \
            --fail-closed \
            --policy policies/ci.yaml \
            --timeout 300 \
            -- ./start-mcp-server.sh
```

### GitLab CI

```yaml
test:
  script:
    - cargo install assay-cli
    - assay sandbox --env-strict --fail-closed -- ./mcp-server
```

---

## Threat Model

### What Assay Sandbox Protects Against

| Threat | Mitigation | Layer |
|--------|------------|-------|
| Credential theft via env vars | Env scrubbing | 1 |
| API key exfiltration | Pattern-based + strict mode | 1 |
| LD_PRELOAD injection | Exec-influence scrubbing | 2 |
| PYTHONPATH hijacking | Exec-influence scrubbing | 2 |
| Reading ~/.ssh/id_rsa | Landlock containment | 3 |
| Writing to /etc | Landlock containment | 3 |
| Symlink escape attacks | Inode-based resolution | 3 |
| Cross-run /tmp pollution | Scoped /tmp | 4 |
| $USER spoofing | Kernel UID | 4 |

### What It Does NOT Protect Against

| Threat | Why |
|--------|-----|
| Kernel exploits | Requires root/CAP_SYS_ADMIN |
| Network exfiltration | Requires `net: block` policy |
| Side-channel attacks | Out of scope for LSM |
| Attacks within allowed paths | By design (allow means allow) |
| Container escape | Use proper containers for that |

### Defense Recommendations

For **development**:
```bash
assay sandbox -- ./mcp-server
```

For **CI/CD**:
```bash
assay sandbox --env-strict -- ./mcp-server
```

For **production** with untrusted code:
```bash
assay sandbox \
  --env-strict \
  --fail-closed \
  --policy policies/locked.yaml \
  -- ./untrusted-server
```

---

## Troubleshooting

### "Degrading to Audit mode"

Your policy has deny-inside-allow conflicts:

```
WARN: Landlock cannot enforce deny inside allowed path
INFO: Degrading to Audit mode (containment disabled)
```

**Fix**: Restructure policy to avoid denying inside allowed paths, or use `--fail-closed` to fail fast.

### "Environment variable X not found"

The var was scrubbed. Use `--env-allow`:

```bash
assay sandbox --env-allow MY_NEEDED_VAR -- ./server
```

### "Permission denied" inside sandbox

The path isn't in your policy's allow list. Add it:

```yaml
fs:
  allow:
    - path: "/needed/path/**"
      read: true
```

### Checking Sandbox Capabilities

```bash
assay doctor
```

Shows:
- Landlock ABI version
- Available security features
- Any degradations or missing capabilities

---

## See Also

- [assay sandbox CLI Reference](../reference/cli/sandbox.md)
- [Environment Filtering Reference](../reference/sandbox-env.md)
- [Sandbox Policies Reference](../reference/sandbox-policies.md)
- [MCP Security Guidance](https://spec.modelcontextprotocol.io/specification/security/)
