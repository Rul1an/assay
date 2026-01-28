# ADR-013: EU AI Act Compliance Pack

## Status

Proposed (January 2026)

## Context

The EU AI Act (Regulation 2024/1689) establishes record-keeping and logging requirements for high-risk AI systems under **Article 12**. Teams deploying AI agents need to demonstrate compliance with these requirements.

Key challenges:
- Article 12 requirements are principles-based, not prescriptive
- Compliance is context-dependent (risk assessment, intended purpose)
- Technical implementation guidance (prEN ISO/IEC 24970) is still draft
- "Passing checks" ≠ "legal compliance" (disclaimer required)

## Decision

We will implement a **compliance pack system** with:
1. **Open baseline pack**: Technical checks mapping to Article 12 requirements
2. **Pack composition**: Multiple packs can be combined
3. **Versioned packs**: Semver for tracking changes
4. **Paid managed packs**: Org-specific tailoring, exceptions, audit reporting

### Pack Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      Compliance Pack System                      │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ Built-in Packs (Open Core)                               │   │
│  │                                                          │   │
│  │  • eu-ai-act@1.0.0     (Article 12 baseline)            │   │
│  │  • sec-17a-4@1.0.0     (Financial records)              │   │
│  │  • soc2@1.0.0          (Security controls)              │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                   │
│                              ▼                                   │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ Custom Packs (Paid)                                      │   │
│  │                                                          │   │
│  │  • myorg/ai-policy@2.1.0  (Org-specific rules)          │   │
│  │  • myorg/exceptions@1.0.0 (Approved deviations)         │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                   │
│                              ▼                                   │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ Lint Engine                                              │   │
│  │                                                          │   │
│  │  assay evidence lint --pack eu-ai-act@1.0.0             │   │
│  │  assay evidence lint --pack eu-ai-act,myorg/exceptions  │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

### EU AI Act Article 12 Mapping

| Article 12 Requirement | Technical Check | Rule ID |
|------------------------|-----------------|---------|
| "Automatic recording of events" | Bundle contains events | `EU12-001` |
| "Over the lifetime of the system" | Retention policy attached | `EU12-002` |
| "Recording of each use period" | `*.started` / `*.finished` event pairs | `EU12-003` |
| "Identify situations presenting risk" | Lint findings present (if any) | `EU12-004` |
| "Post-market monitoring" | Bundle linked to Store (optional) | `EU12-005` |
| "Reference database checked" (biometric) | `data.reference_db` field present | `EU12-006` |
| "Input data that matched" (biometric) | `data.match_result` field present | `EU12-007` |
| "Natural persons verifying results" | `data.reviewer` field present | `EU12-008` |

### Pack Definition Format

```yaml
# packs/eu-ai-act.yaml
name: eu-ai-act
version: 1.0.0
description: EU AI Act Article 12 record-keeping baseline
author: Assay Team
license: Apache-2.0

# Legal disclaimer (REQUIRED)
disclaimer: |
  This pack provides technical checks that map to EU AI Act Article 12
  requirements. Passing these checks does NOT constitute legal compliance.
  Consult legal counsel for compliance determination.

# Minimum assay version
requires:
  assay: ">=2.7.0"

# Rule definitions
rules:
  - id: EU12-001
    severity: error
    description: "Bundle must contain at least one event"
    check:
      type: event_count
      min: 1
    article_ref: "Article 12(1)"

  - id: EU12-002
    severity: warning
    description: "Bundle should have retention metadata"
    check:
      type: manifest_field
      field: "x-assay-retention"
      required: false  # Warning, not error
    article_ref: "Article 12(1)"

  - id: EU12-003
    severity: error
    description: "Use periods must have start and finish events"
    check:
      type: event_pairs
      start_pattern: "*.started"
      finish_pattern: "*.finished"
    article_ref: "Article 12(2)"

  - id: EU12-004
    severity: info
    description: "Risk-related findings should be documented"
    check:
      type: lint_findings
      categories: ["security", "privacy", "safety"]
    article_ref: "Article 12(2)(a)"

# Biometric-specific rules (Annex III, point 1(a))
biometric_rules:
  - id: EU12-006
    severity: error
    description: "Biometric systems must log reference database"
    check:
      type: event_field
      event_type: "assay.biometric.*"
      field: "data.reference_db"
    article_ref: "Article 12(3)(b)"

  - id: EU12-007
    severity: error
    description: "Biometric systems must log match results"
    check:
      type: event_field
      event_type: "assay.biometric.*"
      field: "data.match_result"
    article_ref: "Article 12(3)(c)"

  - id: EU12-008
    severity: warning
    description: "Human reviewer should be identified"
    check:
      type: event_field
      event_type: "assay.*.verified"
      field: "data.reviewer"
    article_ref: "Article 12(3)(d)"
```

### CLI Usage

```bash
# Lint with EU AI Act pack
assay evidence lint bundle.tar.gz --pack eu-ai-act@1.0.0

# Multiple packs (composition)
assay evidence lint bundle.tar.gz --pack eu-ai-act@1.0.0,sec-17a-4@1.0.0

# With org exceptions (paid)
assay evidence lint bundle.tar.gz \
  --pack eu-ai-act@1.0.0 \
  --pack myorg/exceptions@1.0.0

# Output formats
assay evidence lint bundle.tar.gz --pack eu-ai-act --format sarif
assay evidence lint bundle.tar.gz --pack eu-ai-act --format json
assay evidence lint bundle.tar.gz --pack eu-ai-act --format markdown

# Audit report (paid)
assay evidence audit bundle.tar.gz \
  --pack eu-ai-act@1.0.0 \
  --format pdf \
  --out compliance-report.pdf
```

### SARIF Output with Pack Metadata

```json
{
  "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/...",
  "version": "2.1.0",
  "runs": [{
    "tool": {
      "driver": {
        "name": "assay-evidence-lint",
        "version": "2.7.0",
        "rules": [
          {
            "id": "EU12-001",
            "shortDescription": { "text": "Bundle must contain at least one event" },
            "helpUri": "https://docs.assay.dev/packs/eu-ai-act#EU12-001",
            "properties": {
              "pack": "eu-ai-act@1.0.0",
              "article_ref": "Article 12(1)",
              "tags": ["compliance", "eu-ai-act"]
            }
          }
        ]
      },
      "extensions": [{
        "name": "eu-ai-act",
        "version": "1.0.0",
        "properties": {
          "disclaimer": "This pack provides technical checks..."
        }
      }]
    },
    "results": [...]
  }]
}
```

## Open Core vs Paid Split

| Feature | Open Core | Paid |
|---------|-----------|------|
| Built-in packs (`eu-ai-act`, etc.) | ✅ | ✅ |
| `--pack` CLI flag | ✅ | ✅ |
| SARIF/JSON/Markdown output | ✅ | ✅ |
| Pack composition | ✅ | ✅ |
| Custom pack definitions | ✅ | ✅ |
| **Org-hosted pack registry** | ❌ | ✅ |
| **Exception management** | ❌ | ✅ |
| **Audit report generation (PDF)** | ❌ | ✅ |
| **Compliance dashboard** | ❌ | ✅ |
| **Evidence Store linkage** | ❌ | ✅ |
| **Auditor export workflow** | ❌ | ✅ |

### Monetization

| Item | Model |
|------|-------|
| Base packs | Free (open core) |
| Custom pack hosting | Per-pack/month |
| Exception management | Per-org/month |
| Audit reports | Per-report or included in tier |
| Compliance dashboard | Included in Pro/Enterprise |

## Implementation Plan

### Phase 1: Pack Engine (Week 1-2)
- [ ] Pack definition YAML schema
- [ ] Pack loader and validator
- [ ] Built-in `eu-ai-act@1.0.0` pack
- [ ] `--pack` CLI flag for `assay evidence lint`
- [ ] Pack metadata in SARIF output

### Phase 2: Additional Packs (Week 3-4)
- [ ] `sec-17a-4@1.0.0` pack (financial)
- [ ] `soc2@1.0.0` pack (security)
- [ ] Pack composition logic
- [ ] Pack versioning (semver resolution)

### Phase 3: Paid Features (Week 5-6)
- [ ] Org pack registry (S3-backed)
- [ ] Exception workflow
- [ ] PDF audit report generation
- [ ] Evidence Store linkage

## Acceptance Criteria

- [ ] `assay evidence lint --pack eu-ai-act@1.0.0` produces findings
- [ ] SARIF output includes pack metadata and article references
- [ ] Disclaimer is included in all pack outputs
- [ ] Pack composition works (`--pack a,b`)
- [ ] Custom pack definitions can be loaded from file
- [ ] Version mismatch produces clear error

## Consequences

### Positive
- Clear technical mapping to regulatory requirements
- Versioned packs enable tracking of compliance over time
- Composition allows layering org-specific rules
- SARIF integration with existing CI/CD workflows

### Negative
- Risk of "compliance theater" (passing ≠ compliant)
- Pack maintenance burden as regulations evolve
- Legal disclaimer management

### Neutral
- prEN ISO/IEC 24970 may require pack updates when finalized
- Different jurisdictions may need localized packs

## References

- [EU AI Act Article 12](https://artificialintelligenceact.eu/article/12/)
- [EU AI Act Full Text (Regulation 2024/1689)](https://eur-lex.europa.eu/eli/reg/2024/1689/oj)
- [Draft prEN ISO/IEC 24970](https://www.iso.org/standard/79799.html) (AI System Logging)
- [ADR-009: WORM Storage](./ADR-009-WORM-Storage.md)
- [ADR-010: Evidence Store API](./ADR-010-Evidence-Store-API.md)
