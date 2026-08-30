# Security Policy

Assay is security-critical infrastructure for AI agents. We take vulnerabilities seriously.

## Supported Versions

Assay supports the current published release, **v5.5.1**.

Older releases do not carry a maintenance or end-of-life commitment unless a release note says so
explicitly.

## Reporting Vulnerabilities

**Do not report security issues through public GitHub issues.**

Use GitHub's [private vulnerability reporting](https://github.com/Rul1an/assay/security/advisories/new)
form. Do not include secrets in a public issue.

## Threat Model

Assay runs in untrusted environments (CI/CD, agent sandboxes).

### In Scope

| Category | Examples |
|----------|----------|
| **Policy Bypass** | Circumventing `deny` lists, regex constraints |
| **RCE** | Code execution via malicious config/trace |
| **MCP Violations** | Unauthorized tool calls through proxy |
| **Evidence Tampering** | Bundle modification, manifest spoofing |
| **Terminal Injection** | ANSI escape attacks in `evidence explore` |

### Out of Scope

- Physical access attacks
- DoS (lower priority than integrity)
- Social engineering

## Security Features

### Evidence Integrity

- Content-addressed bundle IDs (SHA-256)
- JCS canonicalization (RFC 8785)
- Verification gate before any processing

### Tool Signing (Planned)

- `x-assay-sig` extension field
- Sigstore/Rekor transparency logging
- Trust policy enforcement

### Runtime Isolation

- Landlock (rootless containment)
- eBPF/LSM (kernel enforcement)
- Environment scrubbing

## Supply Chain

| Component | Protection |
|-----------|------------|
| Crates.io | Trusted Publishing (OIDC) |
| PyPI | Trusted Publishing |
| Dependencies | `cargo-deny` audit in CI |
| Releases | GitHub Actions, no manual tokens |

## Local Credential Hygiene

MCP registry token files named `.mcpregistry_*` are local-only secrets. They are
ignored by git and must never be committed, copied into logs, or uploaded as CI
artifacts. If such files may have appeared in shell history, terminal logs, or
shared artifacts, rotate the underlying credentials before continuing.

Run `scripts/ci/check-mcpregistry-secret-hygiene.sh` before publishing changes
that touch registry auth or release workflows. Set
`ASSAY_FAIL_ON_LOCAL_MCPREGISTRY_TOKENS=1` when a hard-fail local preflight is
preferred.
