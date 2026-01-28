# ADR-014: GitHub Action v2 Design

**Status:** Implemented
**Date:** 2026-01-28
**Deciders:** @Rul1an

## Context

The current `assay-action@v1` provides basic coverage checking. The AI-assisted development landscape has shifted significantly:

1. **GitHub SARIF Change (July 2025)**: GitHub stopped merging multiple runs with same tool+category in one SARIF file. Uploads can now fail silently.
2. **Agent-heavy workflows**: More iterations, more supply-chain risk, need for verifiable outputs.
3. **Shift from "test runners" to "quality gates"**: Binary pass/fail → multi-dimensional evaluation.

Two design proposals were evaluated. This ADR captures the combined decision.

## Decision

### Architecture: Separate Repository

**Repository:** https://github.com/Rul1an/assay-action
**Marketplace:** https://github.com/marketplace/actions/assay-ai-agent-security

| Factor | Decision | Rationale |
|--------|----------|-----------|
| Repository | Separate | GitHub Marketplace requires action.yml in root |
| Reference | `Rul1an/assay-action@v2` | Simple, short, marketplace-friendly |
| Composability | Deferred to v2.1 | Simplicity first, sub-actions later |

> **Note:** Initial design was monorepo (`assay/assay-action/`), but GitHub Marketplace doesn't support subdirectory actions for automatic listing. Moved to separate repo for better DX and discoverability.

### Core Capability: Verify + Lint + Diff → SARIF

```yaml
- uses: Rul1an/assay-action@v2
```

**Default behavior (zero-config):**
1. Auto-detect bundles: `**/*.tar.gz` under `.assay/evidence/`
2. `assay evidence verify` all bundles
3. `assay evidence lint --format sarif`
4. Upload SARIF to GitHub Code Scanning
5. Post PR comment (only if findings or delta)
6. Upload artifacts (bundle, lint.json, diff.json)

### SARIF Discipline (Critical)

Per GitHub's July 2025 change:

```yaml
# MUST: unique category per job/matrix combination
category: "assay-${{ github.workflow }}-${{ github.job }}-${{ matrix.os || 'default' }}"
```

- One SARIF run per bundle per job
- Explicit `automationDetails.id` for fingerprint stability
- SARIF 2.1.0 only

### Input Contract

| Input | Type | Default | Description |
|-------|------|---------|-------------|
| `bundles` | glob | `**/*.tar.gz` | Evidence bundle pattern |
| `fail_on` | enum | `error` | `error`, `warn`, `info`, `none` |
| `sarif` | bool | `true` | Upload to Code Scanning |
| `category` | string | auto | SARIF category (auto-generated if omitted) |
| `baseline_dir` | path | - | Path to baseline bundles |
| `baseline_key` | string | - | Key for baseline lookup |
| `write_baseline` | bool | `false` | Write baseline (main branch only) |
| `comment_diff` | bool | `true` | Post PR comment with diff |
| `upload_artifacts` | bool | `true` | Upload bundles + reports |
| `compliance_pack` | string | - | e.g., `eu-ai-act@1.0.0` |

### Output Contract

| Output | Type | Description |
|--------|------|-------------|
| `verified` | bool | All bundles passed verification |
| `findings_error` | int | Count of error-level findings |
| `findings_warn` | int | Count of warn-level findings |
| `sarif_path` | path | Path to generated SARIF |
| `diff_summary` | string | One-line diff summary |

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success, no findings above threshold |
| 1 | Findings exceed `fail_on` threshold |
| 2 | Verification failed (bundle integrity) |
| 3 | Configuration error |

### Artifacts Uploaded

```
assay-evidence-${{ github.run_id }}/
├── bundles/
│   └── *.tar.gz
├── lint.json
├── lint.sarif
├── diff.json
└── summary.md
```

### PR Comment Format

```markdown
## 🛡️ Assay Evidence Report

| Check | Status |
|-------|--------|
| Verified | ✅ 3/3 bundles |
| Lint | ⚠️ 2 warnings |
| Baseline | ✅ No regressions |

<details>
<summary>Diff vs baseline</summary>

| Category | Added | Removed |
|----------|-------|---------|
| Hosts | +1 (api.new.com) | - |
| Files | - | -2 |

</details>

📦 [View artifacts](link) | 🔍 Run locally: `assay evidence explore bundle.tar.gz`
```

### GitHub Job Summary

Always written. Contains:
- Verification status per bundle
- Findings by severity (table)
- Top 3 new hosts/files/processes (if baseline provided)
- Link to artifacts

### Permissions Required

```yaml
permissions:
  contents: read          # Checkout
  security-events: write  # SARIF upload
  pull-requests: write    # PR comment (optional)
```

## Rationale

### Why Combined Approach

| Source | Contribution |
|--------|--------------|
| Analysis 1 | Adoption tiers, MCP integration, monorepo decision |
| Analysis 2 | SARIF discipline, concrete contract, "no noise" principle |

### Why Not Sub-Actions (Yet)

Sub-actions (`/setup`, `/lint`, `/evidence`) add complexity:
- More repos/paths to maintain
- Version matrix explosion
- User confusion on composition

Decision: Ship v2.0 as single action, evaluate sub-actions for v2.1 based on user feedback.

### Why Not Reusable Workflow (Yet)

Reusable workflows are powerful for enterprise standardization but:
- Require `workflow_call` trigger (breaks simple adoption)
- Less flexible for customization
- Can be added later as `assay/workflows/`

## Implementation Phases

### v2.0 (MVP) ✅ Completed

- [x] Zero-config auto-discovery
- [x] SARIF upload with correct category discipline
- [x] PR comment with diff (no noise if clean)
- [x] Baseline regression gate (cache-based)
- [x] GitHub Job Summary
- [x] Artifact upload (with `include-hidden-files` fix for `.assay-reports/`)
- [x] Separate repository for Marketplace publication

### v2.1

- [ ] `assay init` command (generates starter workflow)
- [ ] Compliance pack support (`--pack eu-ai-act@1.0.0`)
- [ ] Coverage badge generation
- [ ] MCP config scanning (`mcp-scan: true`)

### v2.2+

- [ ] Composable sub-actions
- [ ] Sigstore attestation
- [ ] Rekor transparency logging
- [ ] LangSmith/Braintrust trace ingestion

## Consequences

### Positive

- 1-minute adoption for new users
- Native GitHub Security tab integration
- Verified-only diff prevents false positives
- Artifact trail for auditors

### Negative

- Single action = less granular caching
- SARIF category naming requires user awareness for matrix builds

### Risks

| Risk | Mitigation |
|------|------------|
| SARIF rejection | Auto-generate unique category per job |
| Noisy PR comments | Only comment if findings or delta |
| Slow binary download | Cache binary in toolcache pattern |

## References

- [GitHub SARIF Support](https://docs.github.com/en/code-security/code-scanning/integrating-with-code-scanning/sarif-support-for-code-scanning)
- [GitHub Code Scanning July 2025 Changes](https://github.blog/changelog/)
- [Trivy Action (monorepo pattern)](https://github.com/aquasecurity/trivy)
- [Semgrep Action (monorepo pattern)](https://github.com/semgrep/semgrep)
