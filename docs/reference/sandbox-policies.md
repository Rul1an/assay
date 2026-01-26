# Sandbox Policies Reference

Complete reference for Assay sandbox filesystem and network policies.

---

## Overview

Sandbox policies define what the sandboxed process can access:
- **Filesystem**: Which paths can be read, written, or executed
- **Network**: Which connections are allowed (future: egress filtering)

Policies are enforced by **Linux Landlock LSM** at the kernel level.

---

## Policy Schema

```yaml
version: "1.0"
name: "policy-name"

fs:
  allow:
    - path: "/some/path/**"
      read: true
      write: false
      execute: false
  deny:
    - path: "/sensitive/path/**"

net:
  mode: audit  # audit | block | allow
```

---

## Filesystem Rules

### Allow Rules

Specify paths the process can access:

```yaml
fs:
  allow:
    - path: "${CWD}/**"
      read: true
      write: true
      execute: false

    - path: "/usr/lib/**"
      read: true
      execute: true

    - path: "${TMPDIR}/**"
      read: true
      write: true
```

### Permissions

| Permission | Description |
|------------|-------------|
| `read` | Read file contents, list directories |
| `write` | Create, modify, delete files and directories |
| `execute` | Execute files, traverse directories |

Default if not specified: `false`

### Deny Rules

Specify paths to explicitly deny:

```yaml
fs:
  deny:
    - path: "${HOME}/.ssh/**"
    - path: "${HOME}/.aws/**"
    - path: "${HOME}/.gnupg/**"
    - path: "/etc/shadow"
    - path: "/etc/passwd"
```

⚠️ **Important**: Deny rules have limitations. See [Landlock Limitations](#landlock-limitations).

---

## Path Variables

Policies support variable expansion:

| Variable | Expansion | Example |
|----------|-----------|---------|
| `${CWD}` | Current working directory | `/home/user/project` |
| `${HOME}` | User home directory | `/home/user` |
| `${TMPDIR}` | Scoped temp directory | `/tmp/assay-1000-12345` |
| `${USER}` | Current username | `user` |

### Usage

```yaml
fs:
  allow:
    - path: "${CWD}/**"         # /home/user/project/**
    - path: "${HOME}/.config/**" # /home/user/.config/**
    - path: "${TMPDIR}/**"       # /tmp/assay-1000-12345/**
```

### Glob Patterns

| Pattern | Meaning |
|---------|---------|
| `/**` | All files and subdirectories recursively |
| `/*` | Direct children only |
| (none) | Exact path match |

```yaml
fs:
  allow:
    - path: "${CWD}/**"      # Everything under CWD recursively
    - path: "${CWD}/*"       # Only direct children of CWD
    - path: "${CWD}/file.txt" # Exact file only
```

---

## Built-in Policies

### minimal (Default)

Read-only access to current directory, write only to scoped /tmp:

```yaml
version: "1.0"
name: "minimal"

fs:
  allow:
    - path: "${CWD}/**"
      read: true
      write: false
      execute: false
    - path: "${TMPDIR}/**"
      read: true
      write: true
      execute: false

net:
  mode: audit
```

### development

Read/write access to current directory:

```yaml
version: "1.0"
name: "development"

fs:
  allow:
    - path: "${CWD}/**"
      read: true
      write: true
      execute: false
    - path: "${TMPDIR}/**"
      read: true
      write: true
      execute: true
    - path: "/usr/lib/**"
      read: true
      execute: true
    - path: "/lib/**"
      read: true
      execute: true

net:
  mode: audit
```

### mcp-server

Tailored for typical MCP server needs:

```yaml
version: "1.0"
name: "mcp-server"

fs:
  allow:
    - path: "${CWD}/**"
      read: true
      write: false
    - path: "${TMPDIR}/**"
      read: true
      write: true
    - path: "/usr/**"
      read: true
      execute: true
    - path: "/lib/**"
      read: true
      execute: true
    - path: "/etc/ssl/**"
      read: true
    - path: "/etc/ca-certificates/**"
      read: true
  deny:
    - path: "${HOME}/.ssh/**"
    - path: "${HOME}/.aws/**"
    - path: "${HOME}/.gnupg/**"

net:
  mode: audit
```

---

## Network Modes

| Mode | Description |
|------|-------------|
| `audit` | Log connections but don't block (default) |
| `block` | Block all network access |
| `allow` | Allow all network access (no enforcement) |

```yaml
net:
  mode: block  # Fully offline sandbox
```

> **Note**: Fine-grained network rules (egress filtering by IP/port) require Landlock ABI v4+ (Linux 6.7+). Assay will show kernel requirements in `assay doctor`.

---

## Landlock Limitations

### Allow-Only Architecture

Landlock is **allow-only**. You cannot deny a path inside an allowed parent:

```yaml
# ❌ CANNOT BE ENFORCED:
fs:
  allow:
    - path: "${HOME}/**"      # Allow all of home
  deny:
    - path: "${HOME}/.ssh/**" # Try to deny .ssh
```

**Why**: Landlock evaluates from most-specific to least-specific. Once `${HOME}/**` allows access, the kernel permits it. There's no "deny override".

### How Assay Handles Conflicts

When Assay detects a deny-inside-allow conflict:

1. **Warns** about the unenforced deny rule
2. **Degrades to Audit mode** (logs but doesn't enforce)
3. **Shows clear banner** indicating degraded security

```
WARN: Landlock cannot enforce deny inside allowed path:
      /home/user/.ssh (allowed by /home/user)
INFO: Degrading to Audit mode (containment disabled)

Backend: Landlock (Audit)
  FS:  audit (degraded)
```

### Fail-Closed Mode

Use `--fail-closed` to exit instead of degrading:

```bash
assay sandbox --fail-closed --policy my-policy.yaml -- ./server
# ERROR: Policy cannot be fully enforced
# exit 2
```

### Best Practice: Minimal Allows

Avoid conflicts by using specific allow paths:

```yaml
# ✅ Good: No conflicts possible
fs:
  allow:
    - path: "${CWD}/src/**"
    - path: "${CWD}/data/**"
    - path: "${TMPDIR}/**"
  deny:
    - path: "${HOME}/.ssh/**"  # Not inside any allow → works!
```

---

## Policy Examples

### Read-Only Project Access

```yaml
version: "1.0"
name: "read-only"

fs:
  allow:
    - path: "${CWD}/**"
      read: true
      write: false
    - path: "${TMPDIR}/**"
      read: true
      write: true

net:
  mode: audit
```

### Offline Data Processing

```yaml
version: "1.0"
name: "offline-processor"

fs:
  allow:
    - path: "${CWD}/input/**"
      read: true
    - path: "${CWD}/output/**"
      read: true
      write: true
    - path: "${TMPDIR}/**"
      read: true
      write: true

net:
  mode: block  # No network access
```

### CI Pipeline

```yaml
version: "1.0"
name: "ci-locked"

fs:
  allow:
    - path: "${CWD}/**"
      read: true
      write: true
    - path: "${TMPDIR}/**"
      read: true
      write: true
    - path: "/usr/**"
      read: true
      execute: true
    - path: "/lib/**"
      read: true
      execute: true
    - path: "/bin/**"
      read: true
      execute: true

net:
  mode: audit
```

### Security Research (Paranoid)

```yaml
version: "1.0"
name: "paranoid"

fs:
  allow:
    - path: "${TMPDIR}/**"
      read: true
      write: true
  # Nothing else allowed!

net:
  mode: block
```

---

## Policy Validation

### Check Syntax

```bash
assay policy validate my-policy.yaml
```

### Preview Enforcement

```bash
assay sandbox --verbose --policy my-policy.yaml -- true
```

Shows:
- Expanded paths
- Detected conflicts
- Effective rules

---

## Policy Compilation

When `assay sandbox` runs, it:

1. **Parses** the YAML policy
2. **Expands** variables (`${CWD}` → `/home/user/project`)
3. **Canonicalizes** paths (resolves symlinks, `..`, etc.)
4. **Detects conflicts** (deny inside allow)
5. **Builds Landlock ruleset** (allow rules only)
6. **Applies** in pre_exec (after fork, before exec)

### Compilation Errors

| Error | Cause |
|-------|-------|
| `Path not found` | Variable expansion failed or path doesn't exist |
| `Conflict detected` | Deny rule inside allow path |
| `Invalid permission` | Unknown permission type |

---

## Environment Integration

Policies work with environment scrubbing:

```bash
# Policy + strict env
assay sandbox \
  --policy my-policy.yaml \
  --env-strict \
  -- ./server
```

The scoped `/tmp` created by the sandbox is automatically:
- Added to the policy's allow list
- Set as `TMPDIR`, `TMP`, `TEMP`

---

## Diagnostics

### Check Capabilities

```bash
assay doctor
```

Shows:
- Landlock ABI version
- Available filesystem scopes
- Network enforcement availability

### Debug Policy Application

```bash
assay sandbox --verbose --policy my-policy.yaml -- ./cmd
```

Shows:
- Which rules were applied
- Any conflicts or degradations
- Effective security posture

---

## Future: BPF-LSM

For full deny-wins semantics, Assay will support **BPF-LSM** backend:

```yaml
backend: bpf-lsm  # Future

fs:
  allow:
    - path: "${HOME}/**"
  deny:
    - path: "${HOME}/.ssh/**"  # Will be enforced!
```

BPF-LSM can express arbitrary allow/deny logic. Watch `assay doctor` for availability.

---

## See Also

- [assay sandbox CLI Reference](cli/sandbox.md)
- [Sandbox Security Guide](../guides/sandbox-security.md)
- [Environment Filtering Reference](sandbox-env.md)
- [Landlock Documentation](https://docs.kernel.org/userspace-api/landlock.html)
