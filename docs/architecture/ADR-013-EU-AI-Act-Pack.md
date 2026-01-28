# ADR-013: EU AI Act Compliance Pack

## Status

Accepted (January 2026)

Updated with baseline/pro taxonomy per [ADR-016](./ADR-016-Pack-Taxonomy.md).

## Context

The EU AI Act (Regulation 2024/1689) establishes record-keeping and logging requirements for high-risk AI systems under **Article 12**. Teams deploying AI agents need to demonstrate compliance with these requirements.

Key challenges:
- Article 12 requirements are principles-based, not prescriptive
- Compliance is context-dependent (risk assessment, intended purpose)
- Technical implementation guidance (prEN ISO/IEC 24970) is still draft
- "Passing checks" ≠ "legal compliance" (disclaimer required)

## Decision

We implement a **compliance pack system** following the Semgrep open core model:

| Component | License | Description |
|-----------|---------|-------------|
| **Pack Engine** | Apache 2.0 | Loader, composition, SARIF output |
| **Baseline Packs** | Apache 2.0 | `eu-ai-act-baseline`, basic Article 12 checks |
| **Pro Packs** | Commercial | `eu-ai-act-pro`, biometric rules, PDF reports |
| **Managed Workflows** | Commercial | Exceptions, approvals, scheduled scans |

See [ADR-016: Pack Taxonomy](./ADR-016-Pack-Taxonomy.md) for the complete open core split.

## Pack Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      Compliance Pack System                      │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ Baseline Packs (Open Source - Apache 2.0)               │   │
│  │                                                          │   │
│  │  • eu-ai-act-baseline@1.0.0  (Article 12 core)          │   │
│  │  • soc2-baseline@1.0.0       (Control mapping)          │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                   │
│                              ▼                                   │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ Pro Packs (Enterprise - Commercial License)             │   │
│  │                                                          │   │
│  │  • eu-ai-act-pro@1.0.0       (Biometric, PDF reports)   │   │
│  │  • soc2-pro@1.0.0            (Advanced controls)        │   │
│  │  • myorg/exceptions@1.0.0    (Org-specific)             │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                   │
│                              ▼                                   │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ Pack Engine (Open Source)                               │   │
│  │                                                          │   │
│  │  assay evidence lint --pack eu-ai-act-baseline          │   │
│  │  assay evidence lint --pack eu-ai-act-baseline,soc2     │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

## EU AI Act Article 12 Mapping

### Source: Article 12 Requirements

> High-risk AI systems shall technically allow for the automatic recording of events (logs) over the lifetime of the system. Logging capabilities shall provide, at a minimum:
> - (a) recording of events relevant for identifying situations that may result in risk or substantial modification
> - (b) facilitation of post-market monitoring
> - (c) monitoring of the operation of high-risk AI systems

### Baseline Pack Rules (Open Source)

Direct mapping to Article 12(1) and 12(2)(a)(b)(c):

| Rule ID | Article | Check | Description |
|---------|---------|-------|-------------|
| EU12-001 | 12(1) | Event presence | Evidence bundle contains automatically recorded operational events |
| EU12-002 | 12(2)(c) | Operation monitoring | Events include run lifecycle fields (started/finished, status, environment) |
| EU12-003 | 12(2)(b) | Post-market monitoring | Events include correlation IDs (run_id, trace_context, version/build_id) |
| EU12-004 | 12(2)(a) | Risk identification | Events include fields for risk situation identification (policy denials, config changes) |

**Note**: "Retention metadata" is governance/records management, not Article 12 itself. Moved to Pro pack.

### Pro Pack Rules (Enterprise)

| Rule ID | Article | Check | Description |
|---------|---------|-------|-------------|
| EU12-005 | 12(2)(b) | Retention validation | Bundle has retention policy attached |
| EU12-006 | 12(3)(b) | Biometric: reference DB | Biometric systems log reference database |
| EU12-007 | 12(3)(c) | Biometric: match results | Biometric systems log match results |
| EU12-008 | 12(3)(d) | Human reviewer | Human reviewer is identified |

## Pack Engine Specification

### Rule ID Namespacing

To prevent collision when composing packs (`--pack a,b`):

```
Canonical ID: {pack_name}@{pack_version}:{rule_id}
Example:      eu-ai-act-baseline@1.0.0:EU12-001
```

**Collision policy**:
- Same canonical ID from same pack = dedupe
- Same short_id from different packs = both run (canonical IDs differ)
- Same canonical ID from different packs = last wins + warning

### Pack Schema

```yaml
# packs/eu-ai-act-baseline.yaml
name: eu-ai-act-baseline
version: "1.0.0"
kind: compliance          # compliance | security | quality
description: EU AI Act Article 12 record-keeping baseline for high-risk AI systems
author: Assay Team
license: Apache-2.0

# REQUIRED for kind: compliance
disclaimer: |
  This pack provides technical checks that map to EU AI Act Article 12 requirements.
  Passing these checks does NOT constitute legal compliance. Organizations remain
  responsible for meeting all applicable legal requirements. Consult qualified
  legal counsel for compliance determination.

requires:
  assay_min_version: ">=2.9.0"
  evidence_schema_version: "1.0"

rules:
  - id: EU12-001
    severity: error
    description: Evidence bundle contains automatically recorded operational events
    article_ref: "12(1)"
    help_markdown: |
      ## EU AI Act Article 12(1) - Automatic Event Recording

      High-risk AI systems must technically allow for automatic recording of events.
      This check verifies that the evidence bundle contains at least one operational event.
    check:
      type: event_count
      min: 1

  - id: EU12-002
    severity: error
    description: Events include run lifecycle fields for operation monitoring
    article_ref: "12(2)(c)"
    help_markdown: |
      ## EU AI Act Article 12(2)(c) - Operation Monitoring

      Logs must enable monitoring of AI system operation. This check verifies
      events contain lifecycle fields (started/finished events, status, environment).
    check:
      type: event_pairs
      start_pattern: "*.started"
      finish_pattern: "*.finished"

  - id: EU12-003
    severity: warning
    description: Events include correlation IDs for post-market monitoring
    article_ref: "12(2)(b)"
    help_markdown: |
      ## EU AI Act Article 12(2)(b) - Post-Market Monitoring

      Logs must facilitate post-market monitoring. This check verifies events
      contain correlation identifiers (run_id, trace_context, version/build_id).
    check:
      type: event_field_present
      any_of: ["run_id", "traceparent", "build_id", "version"]

  - id: EU12-004
    severity: warning
    description: Events include fields enabling risk situation identification
    article_ref: "12(2)(a)"
    help_markdown: |
      ## EU AI Act Article 12(2)(a) - Risk Identification

      Logs must enable identification of risk situations or substantial modifications.
      This check verifies events contain fields like policy decisions, denials,
      or configuration/policy changes.
    check:
      type: event_field_present
      any_of: ["policy_decision", "denied", "policy_hash", "config_hash", "violation"]
```

### SARIF Output (GitHub-Compatible)

Pack metadata uses `properties` bags (SARIF-standard extensibility):

```json
{
  "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/...",
  "version": "2.1.0",
  "runs": [{
    "tool": {
      "driver": {
        "name": "assay-evidence-lint",
        "version": "2.9.0",
        "properties": {
          "assayPacks": [
            {"name": "eu-ai-act-baseline", "version": "1.0.0", "digest": "sha256:abc..."}
          ]
        },
        "rules": [{
          "id": "eu-ai-act-baseline@1.0.0:EU12-001",
          "shortDescription": {"text": "Evidence bundle contains automatically recorded events"},
          "help": {
            "markdown": "## EU AI Act Article 12(1)\\n\\n**Disclaimer**: This check provides technical verification..."
          },
          "properties": {
            "pack": "eu-ai-act-baseline",
            "pack_version": "1.0.0",
            "short_id": "EU12-001",
            "article_ref": "12(1)"
          }
        }]
      }
    },
    "properties": {
      "disclaimer": "This pack provides technical checks..."
    },
    "results": [{
      "ruleId": "eu-ai-act-baseline@1.0.0:EU12-001",
      "properties": {
        "article_ref": "12(1)"
      }
    }]
  }]
}
```

### CLI Usage

```bash
# Baseline pack (open source)
assay evidence lint bundle.tar.gz --pack eu-ai-act-baseline

# Multiple packs (composition)
assay evidence lint bundle.tar.gz --pack eu-ai-act-baseline,soc2-baseline

# Custom pack from file
assay evidence lint bundle.tar.gz --pack ./my-custom-pack.yaml

# Pro pack (requires enterprise license)
assay evidence lint bundle.tar.gz --pack eu-ai-act-pro

# Audit report (enterprise)
assay evidence audit bundle.tar.gz \
  --pack eu-ai-act-pro \
  --format pdf \
  --out compliance-report.pdf
```

## Implementation Plan

### Phase 1: Pack Engine (P2)
- [x] Pack definition YAML schema with `pack_kind`
- [x] Rule ID namespacing (`{pack}@{version}:{rule_id}`)
- [x] Pack loader with digest computation
- [x] `--pack` CLI flag for `assay evidence lint`
- [x] Pack metadata in SARIF `properties` bags
- [x] Disclaimer enforcement for compliance packs

### Phase 2: EU AI Act Baseline Pack (P2)
- [x] Direct Article 12(1) + 12(2)(a)(b)(c) mapping
- [x] Built-in `eu-ai-act-baseline@1.0.0` pack
- [x] Help markdown with Article references

### Phase 3: Pro Packs (Enterprise)
- [ ] `eu-ai-act-pro@1.0.0` with biometric rules
- [ ] PDF audit report generation
- [ ] Exception workflow support
- [ ] Managed pack registry

## Acceptance Criteria

### Pack Engine (OSS)
- [ ] Rule ID canonical format prevents collisions
- [ ] `pack_kind == compliance` requires disclaimer (hard fail)
- [ ] Pack digest (sha256) in SARIF output
- [ ] `assay_min_version` checked on load
- [ ] SARIF metadata in `properties` (not `tool.extensions`)

### EU AI Act Baseline Pack (OSS)
- [ ] 4 rules with direct Article 12(1) + 12(2)(a)(b)(c) mapping
- [ ] Disclaimer in `help.markdown` + `run.properties`
- [ ] `article_ref` in rule and result `properties`

## Consequences

### Positive
- Legally defensible Article 12 mapping (direct to source)
- Open baseline drives adoption
- Versioned packs enable compliance tracking
- SARIF integration with GitHub Code Scanning

### Negative
- Risk of "compliance theater" (passing ≠ compliant)
- Pack maintenance as regulations evolve
- Clear baseline/pro boundary management

### Neutral
- prEN ISO/IEC 24970 may require pack updates when finalized
- Different jurisdictions may need localized baseline packs

## References

- [EU AI Act Article 12](https://artificialintelligenceact.eu/article/12/)
- [EU AI Act Full Text (Regulation 2024/1689)](https://eur-lex.europa.eu/eli/reg/2024/1689/oj)
- [Draft prEN ISO/IEC 24970](https://www.iso.org/standard/79799.html) (AI System Logging)
- [ADR-016: Pack Taxonomy](./ADR-016-Pack-Taxonomy.md)
- [SARIF 2.1.0 Spec](https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html)
