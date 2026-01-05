# Assay: Two Mental Models

Assay serves two distinct use cases. Understanding which one applies to you will simplify your experience.

## 1. The Validation Tool (Stateless)
**"I just want to check if my agent followed the rules."**

This is how 80% of users start. You have a log file (traces) and a rule file (policy). You want a Pass/Fail result.

- **Primary Command**: `assay validate`
- **Primary Input**: `assay.yaml` (Policy) + `traces.jsonl`
- **State**: None. No database. No history.
- **CI/CD**: Blocks PRs if rules are broken.

**Example**:
```bash
assay validate --config assay.yaml --trace-file runs.jsonl
```

## 2. The Assay Platform (Stateful)
**"I want to track regression over time and compare baselines."**

This is for power users managing long-term agent quality. It requires tracking "Baselines" (gold standard behaviors) and comparing new runs against them.

- **Primary Command**: `assay run`, `assay baseline check`
- **Primary Input**: `assay.yaml` (Config) + Connection to LLM or Trace Store
- **State**: Uses `.assay/eval.db` (SQLite) to store runs and baselines.
- **CI/CD**: Gates deployments based on "No Regression" + "Coverage Maintenance".

**Key Difference**:
The **Platform** treats the "Validation Tool" as just one metric (Compliance) among others (Performance, Cost, Drift).

## Recommendation
Start with **The Validation Tool**. It delivers immediate value with zero infrastructure. Upgrade to **The Platform** only when you need historical analytics or advanced regression testing.
