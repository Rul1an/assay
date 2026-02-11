# ADR-016: Pack Taxonomy (Baseline vs Pro)

## Status

Accepted (January 2026)

## Context

With the introduction of compliance packs ([ADR-013](./ADR-013-EU-AI-Act-Pack.md)), we need to define the open core boundary between free and commercial features.

Key tensions:
- Compliance tools are typically commercial (ComplyAct, OneTrust, etc.)
- Open source compliance adoption requires accessible baseline tooling
- Enterprise value is in workflows, not rule lock-in (Semgrep pattern)
- "Feel-bad free" tiers damage adoption

## Decision

We follow the **Semgrep open core model**:

| Component | License | Rationale |
|-----------|---------|-----------|
| Pack Engine | Apache 2.0 | Distribution mechanism, must be open |
| Baseline Packs | Apache 2.0 | Adoption wedge, basic compliance checks |
| Pro Packs | Commercial | Advanced rules, industry-specific |
| Managed Workflows | Commercial | Exceptions, approvals, dashboards |

**Key principle**: Gate *workflow scale*, not *basic compliance checks*.

## Open Source (Apache 2.0)

### Pack Engine

Everything needed to load, validate, and execute packs:

- YAML schema parser with `pack_kind` (compliance/security/quality)
- Rule ID namespacing: `{pack}@{version}:{rule_id}`
- Pack composition: `--pack a,b` with strict collision handling (hard-fail for compliance packs)
- Version resolution: `assay_min_version`, `evidence_schema_version`
- Pack digest: SHA256 for supply chain integrity
- SARIF output with `properties`-based metadata
- Disclaimer enforcement for compliance packs

### Baseline Packs

Basic compliance checks that map directly to regulatory requirements:

| Pack | Description | Rules |
|------|-------------|-------|
| `eu-ai-act-baseline` | Article 12(1) + 12(2)(a)(b)(c) | EU12-001 through EU12-004 |
| `soc2-baseline` | Basic control mapping | (Future) |

**Baseline pack criteria**:
- Direct mapping to source regulation text
- No proprietary interpretation
- Disclaimer prominently included
- Apache 2.0 licensed

## Enterprise (Commercial)

### Pro Packs

Advanced compliance rules requiring domain expertise. Pro packs provide **assurance depth** (not just extra rules): coverage, consistency, timeliness checks; maintained mappings to frameworks; auditor-friendly reporting. Rules alone are copyable; workflow, integrations, and maintained mappings are not.

| Pack | Description | Rules |
|------|-------------|-------|
| `eu-ai-act-pro` | Retention (Art 19), biometric rules (Art 12(3)) | EU19-001, EU12-005 through EU12-008 |
| `soc2-pro` | Advanced control mapping | (Future) |
| `hipaa-pro` | Healthcare compliance | (Future) |

### Managed Workflows

Org-scale governance features:

- Exception approval workflows
- **Policy exceptions (waivers)**: Expiry, owner, rationale; audit trail for compliance deviations
- Scheduled compliance scans
- PDF audit report generation
- **Auditor Portal**: Read-only export of packs + results + fingerprints; "audit-ready bundles" for external auditors (when Managed Store exists)
- Managed pack registry (org namespaces)
- Pack development services (SOW)
- Compliance dashboards

## Pack Schema Specification

### Required Fields

```yaml
name: string          # Pack identifier (e.g., "eu-ai-act-baseline")
version: string       # Semver (e.g., "1.0.0")
kind: enum            # compliance | security | quality
description: string   # Human-readable description
author: string        # Pack author
license: string       # SPDX identifier
source_url: string    # Primary source URL (e.g., EUR-Lex for EU regulations)

# REQUIRED if kind == "compliance"
disclaimer: string    # Legal disclaimer text

requires:
  assay_min_version: string         # Semver constraint (e.g., ">=2.9.0")
  evidence_schema_version: string   # Schema version (e.g., "1.0")

rules: []             # Array of rule definitions
```

### Rule Definition

```yaml
rules:
  - id: string              # Short rule ID (e.g., "EU12-001")
    severity: enum          # error | warning | info
    description: string     # One-line description
    article_ref: string     # Regulatory reference (optional)
    help_markdown: string   # Detailed help text
    check:
      type: string          # Check type (event_count, event_pairs, event_field_present, etc.)
      # Type-specific fields...
```

### Rule ID Canonical Format

To prevent collisions in pack composition:

```
Canonical:  {pack_name}@{pack_version}:{rule_id}
Example:    eu-ai-act-baseline@1.0.0:EU12-001
```

Used in SARIF `reportingDescriptor.id` for stable fingerprints.

### Pack Digest (Normative)

SHA256 of JCS-canonical pack content for supply chain integrity:

```
pack_digest = sha256( JCS( JSON( parse_yaml(pack_file) ) ) )
Format: sha256:{hex_digest}
```

**Algorithm**:
1. Parse YAML pack file into native data structure
2. Validate against pack schema (unknown fields MUST cause error)
3. Serialize to JSON (only known schema fields)
4. Apply JCS canonicalization (RFC 8785)
5. Compute SHA-256 hash

**Unknown fields policy**: YAML files with fields not defined in the pack schema MUST fail validation. This prevents "invisible" metadata injection that wouldn't be reflected in the digest.

Included in SARIF `tool.driver.properties.assayPacks[].digest`.

## SARIF Output Specification

Pack metadata uses SARIF-standard `properties` bags (not `tool.extensions`):

```json
{
  "tool": {
    "driver": {
      "properties": {
        "assayPacks": [{"name": "...", "version": "...", "digest": "..."}]
      },
      "rules": [{
        "id": "{pack}@{version}:{rule_id}",
        "properties": {
          "pack": "...",
          "pack_version": "...",
          "short_id": "...",
          "article_ref": "..."
        }
      }]
    }
  },
  "results": [{
    "properties": {
      "article_ref": "..."
    }
  }]
}
```

**Rationale**: GitHub Code Scanning uses SARIF 2.1.0 subset. `properties` bags are the SARIF-standard extensibility mechanism and are reliably passed through.

## Stability Policy

### Pack Schema v1

- Breaking changes require major version bump
- Deprecations announced 6 months in advance
- Compliance packs cannot break monthly (audit trails must be reproducible)

### Baseline Pack Updates

- Security fixes: immediate release
- Regulatory changes: coordinated with enforcement dates
- New rules: minor version bump
- Rule removal: major version bump with deprecation notice

## Licensing

### Baseline Packs

```yaml
license: Apache-2.0
```

### Pro Packs

```yaml
license: Assay-Enterprise-1.0
```

License file in pack directory with terms.

## Collision Policy

Pack composition (`--pack a,b`) collision handling:

| Scenario | `kind: compliance` | `kind: security/quality` |
|----------|-------------------|--------------------------|
| Same canonical ID from same pack | Dedupe | Dedupe |
| Same short_id from different packs | Both run | Both run |
| Same canonical ID from different packs | **Hard fail** | Last wins + warning |

**Rationale**: Compliance tooling must not silently change behavior based on pack order. Use explicit `overrides:` mechanism (future) for intentional modifications.

## Future Extensions

### Pack Signing

Packs are signable artefacts using the same trust policy model as tool signing:

- `x-assay-sig` field in pack YAML (or detached `.sig` file)
- Same Ed25519 + DSSE PAE encoding as [SPEC-Tool-Signing-v1](./SPEC-Tool-Signing-v1.md)
- Managed pack registry enforces signature verification

### Override Mechanism

Explicit rule modification without collision:

```yaml
overrides:
  - rule: eu-ai-act-baseline@1.0.0:EU12-003
    severity: error  # Escalate from warning
    justification: "Org policy requires correlation IDs"
```

## Consequences

### Positive

- Clear open/commercial boundary
- Baseline packs drive adoption
- Enterprise value in workflows, not rule lock-in
- Reproducible audit trails with versioned packs

### Negative

- Baseline pack maintenance burden
- Must ensure baseline is "good enough" to be useful
- Clear boundary may be challenged by users wanting more free

### Mitigations

- Baseline directly maps to regulation source (hard to argue)
- Pro adds domain expertise and workflows (clear value-add)
- Pack digest ensures reproducibility regardless of tier

## References

### Related ADRs
- [ADR-013: EU AI Act Compliance Pack](./ADR-013-EU-AI-Act-Pack.md)
- [SPEC-Tool-Signing-v1](./SPEC-Tool-Signing-v1.md)

### Open Core Patterns
- [Semgrep Licensing](https://semgrep.dev/docs/licensing)
- [OPA/Styra Open Core Model](https://www.styra.com/open-policy-agent/)

### Standards
- [SARIF 2.1.0 Properties](https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html#_Toc34317448)
- [RFC 8785 - JCS](https://datatracker.ietf.org/doc/html/rfc8785) — JSON Canonicalization Scheme
