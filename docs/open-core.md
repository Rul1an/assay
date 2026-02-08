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
- Baseline packs are community-extensible; enterprise packs add vertical depth
- Clear boundary: free for evaluation, paid for scale

**For enterprises:**
- Procurement-friendly: core is auditable OSS
- Value in content and workflows, not engine lock-in
- Support and SLAs for commercial features

## Comparison

### vs Compliance SaaS

| | Assay | Typical Compliance SaaS |
|--|-------|-------------------------|
| Engine | OSS | Closed |
| Baseline rules | OSS | Closed/Trial |
| Evidence format | Open standard | Proprietary |
| Release provenance | SLSA (planned) | Varies |
| Self-hosted | Yes | Usually no |
| Audit trail | Yours | Vendor-held |

### vs Agent CI/CD Tools (Feb 2026 landscape)

| | Assay | Agent CI | LangSmith | Dagger |
|--|-------|----------|-----------|--------|
| Focus | Governance + audit | Eval-as-service | Observability + evals | Agentic runtime |
| Deterministic replay | Yes | No | No | No |
| Evidence bundles (tamper-evident) | Yes | No | No | No |
| Compliance packs | Yes (open + commercial) | No | No | No |
| Policy-as-code enforcement | Yes | Evals only | Evals only | Constrained tooling |
| PR gates | Yes (SARIF + comments) | Yes | Yes | Yes |
| Self-hosted | Yes | No (SaaS) | Partial | Yes |
| Framework dependency | None (framework-agnostic) | Framework integrations | LangChain-native | Dagger SDK |

## References

- [ADR-016: Pack Taxonomy](architecture/ADR-016-Pack-Taxonomy.md) — Formal open core boundary definition
- [packs/README.md](https://github.com/Rul1an/assay/tree/main/packs) — Pack directory structure
- [Enterprise Contact](https://getassay.dev/enterprise) — Pricing and custom packs
