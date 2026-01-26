# Sandbox Concepts

Understanding Assay's security sandbox architecture.

---

## What is the Sandbox?

The Assay sandbox is a **security boundary** that isolates MCP servers and AI agents from your system. It restricts what the sandboxed process can:

- **See** (environment variables)
- **Read/Write** (filesystem paths)
- **Connect to** (network endpoints)

---

## Why Sandbox MCP Servers?

MCP servers execute code that you may not fully trust:

```
┌──────────────────────────────────────────────────┐
│  Your AI Agent                                   │
│                                                  │
│  "Use the filesystem MCP server to read         │
│   the project files and help me refactor"       │
└──────────────────────────────────────────────────┘
                       │
                       ▼
┌──────────────────────────────────────────────────┐
│  MCP Server (filesystem)                         │
│                                                  │
│  • Could read ~/.ssh/id_rsa                     │
│  • Could read ~/.aws/credentials                │
│  • Could access GITHUB_TOKEN env var            │
│  • Could write malicious files                  │
│  • Could exfiltrate data via network            │
└──────────────────────────────────────────────────┘
```

Even "trusted" MCP servers can be:
- **Compromised** via supply chain attacks (npm/pypi packages)
- **Tricked** by prompt injection into malicious behavior
- **Buggy** and accidentally expose sensitive data

---

## Security Layers

Assay implements **defense in depth** with four security layers:

### Layer 1: Environment Scrubbing

**Threat**: Credential theft via `process.env` / `os.environ`

**Mitigation**: Remove sensitive variables before process starts

```
Before scrubbing:
  AWS_SECRET_ACCESS_KEY=AKIAXXXXXXXXXX
  GITHUB_TOKEN=ghp_xxxxxxxxxxxx
  OPENAI_API_KEY=sk-xxxxxxxxxx
  PATH=/usr/bin:/bin
  HOME=/home/user

After scrubbing:
  PATH=/usr/bin:/bin
  HOME=/home/user
  TMPDIR=/tmp/assay-1000-12345
```

### Layer 2: Execution Influence Protection

**Threat**: Code injection via `LD_PRELOAD`, `PYTHONPATH`

**Mitigation**: Strip all execution-modifying variables

```
Removed:
  LD_PRELOAD=/tmp/evil.so        # Would inject code
  PYTHONPATH=/tmp/evil           # Would load malicious modules
  NODE_OPTIONS=--require=evil.js # Would run attacker code
```

### Layer 3: Filesystem Containment

**Threat**: Reading secrets, writing malware, escaping project dir

**Mitigation**: Kernel-enforced path restrictions (Landlock LSM)

```
Allowed:
  /home/user/project/**   (read)
  /tmp/assay-1000-12345/** (read+write)

Denied (kernel blocks):
  /home/user/.ssh/**
  /home/user/.aws/**
  /etc/shadow
  /*  (anything not explicitly allowed)
```

### Layer 4: Scoped /tmp Isolation

**Threat**: Cross-process attacks via shared `/tmp`

**Mitigation**: Per-run isolated temp directory

```
Standard /tmp:
  /tmp/                    # World-readable, shared
  /tmp/config.json         # Any process can read/write

Scoped /tmp:
  /tmp/assay-1000-12345/   # UID-1000, PID-12345 only
  mode: 0700               # Owner-only access
```

---

## Landlock LSM

Assay uses **Linux Landlock** for filesystem containment.

### What is Landlock?

Landlock is a Linux Security Module (LSM) that:
- Runs in **kernel space** (cannot be bypassed from userspace)
- Applies to the process and all its children
- Is **unprivileged** (no root required)
- Survives exec() (restrictions persist)

### How It Works

```
┌─────────────────────────────────────────┐
│ User Space                              │
│                                         │
│  ┌─────────────────────────────────┐   │
│  │ Sandboxed Process               │   │
│  │ open("/home/user/.ssh/id_rsa")  │   │
│  └──────────────┬──────────────────┘   │
│                 │                       │
│                 │ syscall               │
└─────────────────┼───────────────────────┘
                  │
┌─────────────────┼───────────────────────┐
│ Kernel Space    ▼                       │
│                                         │
│  ┌─────────────────────────────────┐   │
│  │ Landlock LSM                    │   │
│  │                                 │   │
│  │ Check: Is /home/user/.ssh/*    │   │
│  │        in allowed paths?        │   │
│  │                                 │   │
│  │ Result: NO → return -EACCES    │   │
│  └─────────────────────────────────┘   │
│                                         │
└─────────────────────────────────────────┘
```

### Landlock Limitations

Landlock is **allow-only**. Once a path is allowed, you cannot deny a subpath:

```yaml
# This CANNOT be enforced:
allow: /home/**
deny:  /home/.ssh/**   # ← Ignored by Landlock!
```

Assay detects this and either:
- **Warns** and degrades to audit mode (default)
- **Exits** with error (`--fail-closed`)

---

## Threat Model

### What the Sandbox Protects Against

| Threat | Layer | Example |
|--------|-------|---------|
| Credential theft | 1 | Exfiltrating `GITHUB_TOKEN` |
| API key leakage | 1 | Sending `OPENAI_API_KEY` to attacker |
| Library injection | 2 | `LD_PRELOAD=/tmp/keylogger.so` |
| Module hijacking | 2 | `PYTHONPATH=/tmp/evil` |
| SSH key theft | 3 | Reading `~/.ssh/id_rsa` |
| AWS creds access | 3 | Reading `~/.aws/credentials` |
| System file access | 3 | Reading `/etc/shadow` |
| Temp file attacks | 4 | Writing to shared `/tmp` |
| Process pollution | 4 | Interfering with other sandboxes |

### What the Sandbox Does NOT Protect Against

| Threat | Why | Mitigation |
|--------|-----|------------|
| Kernel exploits | Requires root/CAP_SYS_ADMIN | Keep kernel updated |
| Network exfil | Requires `net: block` policy | Enable network blocking |
| Side channels | Out of scope for LSM | Physical isolation |
| In-scope attacks | By design (allow = allow) | Minimize allow paths |
| Container escape | Different threat model | Use proper containers |

---

## Enforcement Modes

### Containment (Default)

Full kernel enforcement. Blocked operations return `-EACCES`:

```
Backend: Landlock (Containment)
  FS:  contain
  Net: audit
```

### Audit (Degraded)

Logging only, no blocking. Used when policy has conflicts:

```
Backend: Landlock (Audit)
  FS:  audit (degraded)
  Net: audit
⚠ Degradations: 1 (Landlock conflict)
```

### Fail-Closed

Exit immediately if full containment isn't possible:

```bash
assay sandbox --fail-closed -- ./server
# exit 2 if policy can't be enforced
```

---

## Process Lifecycle

```
1. Parse policy
   ↓
2. Expand variables (${CWD}, ${HOME}, ...)
   ↓
3. Canonicalize paths (resolve symlinks)
   ↓
4. Detect conflicts (deny inside allow)
   ↓
5. Create scoped /tmp
   ↓
6. Filter environment variables
   ↓
7. Build Landlock ruleset
   ↓
8. fork()
   ↓
9. [Child] Apply Landlock (pre_exec)
   ↓
10. [Child] exec(command)
   ↓
11. [Parent] Wait for child
   ↓
12. Cleanup scoped /tmp
```

---

## Pre-exec Safety

The sandbox applies Landlock in `pre_exec`, which runs after `fork()` but before `exec()`. This is an **async-signal-safe context** where:

- ❌ No heap allocations
- ❌ No locks
- ❌ No panics/unwinding
- ✅ Only syscalls allowed

Assay prepares everything before fork and only performs raw syscalls in pre_exec:

```rust
// Parent (before fork): all allocations here
let ruleset = build_landlock_ruleset(&policy);

// Child (pre_exec): syscalls only
unsafe {
    prctl(PR_SET_NO_NEW_PRIVS, 1);
    landlock_restrict_self(ruleset_fd);
}
```

---

## See Also

- [assay sandbox CLI](../reference/cli/sandbox.md)
- [Sandbox Security Guide](../guides/sandbox-security.md)
- [Environment Filtering](../reference/sandbox-env.md)
- [Sandbox Policies](../reference/sandbox-policies.md)
