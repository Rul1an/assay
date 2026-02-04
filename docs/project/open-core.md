# Open Core Model

Assay follows the **open core model**: the engine and baseline compliance packs are open source,
while enterprise packs and managed workflows are commercial.

## What's Open Source

Everything needed to create, verify, lint, and analyze evidence locally.

| Category | Components | License |
|----------|------------|---------|
| **CLI** | `export`, `verify`, `lint`, `diff`, `explore`, `show` | MIT |
| **Evidence Contract** | Schema v1, JCS canonicalization, content-addressed IDs | MIT |
| **Pack Engine** | Pack loader, composition, SARIF output, digest verification | MIT |
| **Baseline Packs** | `eu-ai-act-baseline`, `soc2-baseline` (coming soon) | Apache-2.0 |
| **BYOS Storage** | `push`, `pull`, `list` to S3/Azure/GCS/local | MIT |
| **Tool Signing** | Ed25519 local key signing and verification | MIT |
| **Mandate Evidence** | Mandate types, signing, runtime enforcement | MIT |
| **GitHub Action** | Verify/lint/SARIF/attestation wiring | MIT |
| **Python SDK** | `AssayClient`, pytest plugin | MIT |

## What's Commercial

Governance workflows and premium compliance content for organizations.

| Category | Components |
|----------|------------|
| **Pro Compliance Packs** | `eu-ai-act-pro`, `soc2-pro`, `hipaa-pro`, industry packs |
| **Advanced Signing** | Sigstore keyless, transparency log verification |
| **Managed Workflows** | Exception approvals, scheduled scans, dashboards |
| **SIEM Connectors** | Splunk, Sentinel, Datadog, OTel pipeline templates |
| **Managed Storage** | WORM retention, legal hold, compliance attestation |
| **Identity & Access** | SSO/SAML/SCIM, RBAC, teams |

## Pack Licensing

| Pack Type | License | Distribution |
|-----------|---------|--------------|
| Baseline packs (`packs/open/`) | Apache-2.0 | Included in repo |
| Enterprise packs | Commercial | Via Assay Registry |
| Custom packs | Your choice | Your distribution |

## Why This Model

**For users:**
- Audit the code that handles your evidence (trust layer)
- No vendor lock-in on basic compliance checks
- Clear boundary: free for evaluation, paid for scale

**For enterprises:**
- Procurement-friendly: core is auditable OSS
- Value in content and workflows, not engine lock-in
- Support and SLAs for commercial features

## Comparison

| | Assay | Typical Compliance SaaS |
|--|-------|-------------------------|
| Engine | OSS | Closed |
| Baseline rules | OSS | Closed/Trial |
| Evidence format | Open standard | Proprietary |
| Self-hosted | Yes | Usually no |
| Audit trail | Yours | Vendor-held |

## References

- [ADR-016: Pack Taxonomy](architecture/ADR-016-Pack-Taxonomy.md) — Formal open core boundary definition
- [packs/README.md](https://github.com/Rul1an/assay/tree/main/packs) — Pack directory structure
- [Enterprise Contact](https://getassay.dev/enterprise) — Pricing and custom packs
