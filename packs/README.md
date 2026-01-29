# Compliance Packs

Assay uses compliance packs for regulatory and security rule evaluation.

## Structure

```
packs/
├── open/                    # Open source packs (Apache-2.0)
│   └── eu-ai-act-baseline/
│       ├── pack.yaml
│       ├── LICENSE
│       └── README.md
└── enterprise/              # Enterprise packs (via registry)
    └── README.md
```

## Open Source Packs

Open packs are included in the Assay distribution and can be used freely.

| Pack | License | Description |
|------|---------|-------------|
| `eu-ai-act-baseline` | Apache-2.0 | EU AI Act Article 12 baseline |
| `soc2-baseline` | Apache-2.0 | SOC 2 baseline (coming soon) |

**Usage:**

```bash
assay evidence lint --pack eu-ai-act-baseline bundle.tar.gz
```

## Enterprise Packs

Enterprise packs provide advanced compliance rules, industry-specific controls,
and extended coverage. Available via Assay Registry.

See [enterprise/README.md](./enterprise/README.md) for details.

## Custom Packs

Create your own packs using the same YAML schema:

```yaml
name: my-org-rules
version: "1.0.0"
kind: security
description: Organization-specific security rules
author: My Org
license: Proprietary

rules:
  - id: ORG-001
    severity: error
    description: Custom rule
    check:
      type: event_count
      min: 1
```

**Usage:**

```bash
assay evidence lint --pack ./my-org-rules.yaml bundle.tar.gz
```

## Pack Schema

See [SPEC-Pack-Engine-v1](../docs/architecture/SPEC-Pack-Engine-v1.md) for the complete pack schema specification.

## Open Core Model

Assay follows the open core model:

- **Engine + baseline packs** = Open Source (Apache-2.0)
- **Pro packs + managed workflows** = Enterprise

See [ADR-016: Pack Taxonomy](../docs/architecture/ADR-016-Pack-Taxonomy.md) for the formal definition.
