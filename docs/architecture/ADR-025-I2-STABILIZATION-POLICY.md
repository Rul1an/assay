# ADR-025 I2 Stabilization Policy (v1)

## Intent
Harden the ADR-025 I2 closure/release integration with minimal blast radius:
- Improve diagnosability and contract enforcement in scripts
- Add deterministic tests/fixtures for known failure modes
- Keep workflows unchanged in stabilization PRs (no PR-lane impact)

## Scope (stabilization)
In-scope:
- `scripts/ci/adr025-closure-release.sh` hardening (debug output, stricter parsing)
- Additive tests and fixtures
- Docs/runbook updates to match actual behavior
- Reviewer gates for A/B/C stabilization slices

Out-of-scope:
- Any `.github/workflows/*` edits
- Any new product features in CLI / Rust crates
- Any PR required-check / branch-protection changes
- OTel bridge work (I3)

## Contracts to preserve (must not change)
### Exit contract
- 0: pass/attach/off
- 1: policy fail (only blocks in enforce mode)
- 2: measurement/contract fail (missing artifact/json, parse errors, mismatched schema)

### Gate modes
- `off|attach|warn|enforce`
Default remains `attach` (I2 Step4A).

### Artifact contracts (current)
- Nightly closure artifact:
  - workflow: `adr025-nightly-closure.yml`
  - artifact: `adr025-closure-report`
  - files: `closure_report_v1.json`, `closure_report_v1.md` (retention 14 days)

### Policy contract (current)
- `schemas/closure_release_policy_v1.json`
- Minimum required keys: `score_threshold` (and any other v1 fields already in use)

## Classifier / schema mapping rules (freeze for stabilization)
- `closure_report_v1.json` must have:
  - `schema_version == "closure_report_v1"`
  - numeric `score`
- `violations` must be either:
  - array of objects, or
  - null/missing treated as empty
- Any deviation in schema_version or non-numeric score is measurement/contract failure (exit 2).

## Hardening goals (PR-B)
- More actionable errors on:
  - missing GH_TOKEN
  - no runs found for nightly workflow
  - artifact download failure / missing files
- Explicit type-checks on key JSON fields (`score`, `violations`)
- No change to mode semantics or exit codes

## Test matrix (PR-B)
Minimum cases:
- attach mode + missing artifact => exit 0 (non-blocking), with clear log
- warn mode + missing artifact => exit 0 with WARN log
- enforce mode + missing artifact => exit 2
- enforce mode + score below threshold => exit 1
- enforce mode + schema mismatch => exit 2
- violations type edge cases (null / missing / wrong type)
