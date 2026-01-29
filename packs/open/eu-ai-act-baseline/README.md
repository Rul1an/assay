# EU AI Act Baseline Pack

**License:** Apache-2.0
**Version:** 1.0.0
**Scope:** Article 12 record-keeping requirements

## Overview

This pack provides technical checks mapped to EU AI Act Article 12 requirements for high-risk AI systems. It verifies that evidence bundles contain the minimum fields needed for regulatory record-keeping.

## Rules

| Rule ID | Article | Severity | Description |
|---------|---------|----------|-------------|
| EU12-001 | 12(1) | error | Evidence contains automatically recorded events |
| EU12-002 | 12(2)(c) | error | Events include lifecycle fields for operation monitoring |
| EU12-003 | 12(2)(b) | warning | Events include correlation IDs for post-market monitoring |
| EU12-004 | 12(2)(a) | warning | Events include fields for risk situation identification |

## Usage

```bash
assay evidence lint --pack eu-ai-act-baseline bundle.tar.gz
```

Or with other packs:

```bash
assay evidence lint --pack eu-ai-act-baseline,soc2-baseline bundle.tar.gz
```

## SARIF Output

Findings include `article_ref` in the `properties` bag for audit trails:

```json
{
  "ruleId": "eu-ai-act-baseline@1.0.0:EU12-001",
  "properties": {
    "article_ref": "12(1)"
  }
}
```

## Disclaimer

This pack provides technical checks that map to EU AI Act Article 12 requirements.
**Passing these checks does NOT constitute legal compliance.**

Organizations remain responsible for meeting all applicable legal requirements,
including but not limited to:
- Risk assessment
- Conformity assessment
- Ongoing monitoring obligations

Consult qualified legal counsel for compliance determination.

## Reference

- [EU AI Act (Regulation 2024/1689)](https://eur-lex.europa.eu/eli/reg/2024/1689/oj)
- [Article 12 - Record-keeping](https://eur-lex.europa.eu/eli/reg/2024/1689/oj#d1e3029-1-1)

## License

Apache-2.0 — see [LICENSE](./LICENSE)
