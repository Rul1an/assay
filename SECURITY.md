# Security Policy

Assay is security-critical infrastructure for AI agents. We take vulnerabilities seriously.

## Supported Versions

| Version | Status |
|---------|--------|
| **v2.x** | ✅ Supported |
| v1.x | ⚠️ Maintenance only |
| v0.x | ❌ EOL |

## Reporting Vulnerabilities

**Do not report security issues through public GitHub issues.**

Report privately:
- **Email**: security@assay.dev
- **GitHub**: Use "Report a vulnerability" tab

Response time: 24 hours acknowledgment, 72 hours triage.

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
