# Security Policy

Assay is security-critical infrastructure for AI agents. Response-time targets,
disclosure, and CVE policy are in [Response targets](#response-targets).

## Supported Versions

Assay supports the current published release, **v6.1.2**.

Older releases do not carry a maintenance or end-of-life commitment unless a release note says so
explicitly.

## Reporting Vulnerabilities

**Do not report security issues through public GitHub issues.**

Use GitHub's [private vulnerability reporting](https://github.com/Rul1an/assay/security/advisories/new)
form. Do not include secrets in a public issue.

If you have heard nothing after 5 business days, comment on that advisory to
re-send, and send a private DM to an Assay maintainer in Discord (see
[Community](docs/COMMUNITY.md)). Do not file a public issue and do not paste the
report into a public Discord channel.

## Response targets

These are targets, not a measured SLA. One maintainer owns the queue.
Acknowledgement and assessment windows are commitments; no external report has
arrived yet, so those numbers are not measurements. Business days are
Monday-Friday in Europe/Amsterdam, excluding Dutch public holidays. Fix and
disclosure periods are calendar days from receipt of the report.

Fixes ship as a patch release of the supported version.

| Step | Target |
|------|--------|
| Acknowledgement | 3 business days; 1 business day if the reporter says the issue is actively exploited |
| Initial assessment (accepted or not, severity, affected versions) | 7 business days |
| Fixed release or documented mitigation | See the table below |

| Severity | Calendar days from receipt to a fixed release or documented mitigation |
|----------|---------------------------------------------------------------------|
| Critical | 14 |
| High | 30 |
| Medium | 60 |
| Low | 90 |

### Disclosure

A GitHub Security Advisory is published when the fixed release is available.
A CVE is requested for every advisory rated medium or higher. The reporter is
credited unless they decline. Each advisory states when the report arrived
and when the fix shipped.

Coordinated disclosure deadline: 90 calendar days after the report, or 7
calendar days if the issue is actively exploited. The reporter may publish
after that deadline. We may ask for up to 14 more days; the reporter may refuse.

### Missed target

If a target will be missed, the reporter is told in the advisory before the
original date passes, with the reason and a new date. A missed target never
extends the coordinated-disclosure deadline without the reporter's agreement.

## Severity

Severity is a response-priority label for the property that broke. It is not
a trust score or a whole-action verdict.

| Severity | Property | Examples |
|----------|----------|----------|
| Critical | Evidence integrity or verifier correctness | Evidence forgery; verifier bypass of recorded hashes or `VerifyLimits` |
| High | Policy enforcement | Policy bypass through the MCP proxy or kernel/LSM enforcement; RCE via malicious config or trace |
| Medium | Isolation of rendered output | Terminal injection in `evidence explore` |
| Low | Defense in depth with no integrity or enforcement break | Issues that do not break a listed in-scope property |

## Threat Model

Assay runs in untrusted environments (CI/CD, agent sandboxes).

### In Scope

| Category | Examples |
|----------|----------|
| **Policy Bypass** | Circumventing `deny` lists, regex constraints |
| **RCE** | Code execution via malicious config/trace |
| **MCP Violations** | Unauthorized tool calls through proxy |
| **Evidence Tampering** | Bundle modification, manifest spoofing |
| **Limit Bypass** | Circumventing evidence-bundle `VerifyLimits` (compressed size, decompression, event count, and related ceilings) |
| **Terminal Injection** | ANSI escape attacks in `evidence explore` |

### Out of Scope

- Physical access attacks
- Availability impact from heavy but valid input that stays within documented resource limits
- Social engineering

## Security Features

### Evidence Integrity

- Content-addressed bundle IDs (SHA-256)
- JCS canonicalization (RFC 8785)
- Verification gate before any processing

### Tool Signing

Ships in `assay mcp tool keygen`, `sign`, and `verify`:

- `x-assay-sig` Ed25519 signatures over JCS-canonical tool JSON, using DSSE
  Pre-Authentication Encoding
- Verification against an explicit public key (`--pubkey`) or a YAML trust
  policy (`--trust-policy`: `require_signed`, `trusted_key_ids`, `trusted_keys`)

Not in that CLI (still planned):

- Sigstore / Rekor transparency logging for tool signatures
- Keyless / Fulcio signing of tool definitions

### Runtime Isolation

- Landlock (rootless containment)
- eBPF/LSM (kernel enforcement)
- Environment scrubbing

## Verify a release

Do not restate the runbooks here. Offline provenance verification is
[Release Proof Kit](docs/security/RELEASE-PROOF-KIT.md). The published CycloneDX
SBOM, provenance JSON, and proof-kit assets are listed under Verification in
[Release Process](docs/reference/release.md).

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
