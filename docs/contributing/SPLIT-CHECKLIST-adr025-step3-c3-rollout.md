# ADR-025 Step3 C3 Rollout Closure Checklist

## Scope guardrails (hard)
- [ ] No `pull_request` triggers added to ADR-025 workflows
- [ ] No required-check / branch-protection behavior changed
- [ ] All actions in ADR-025 workflows are SHA-pinned
- [ ] Nightly lanes remain informational (`continue-on-error: true`)
- [ ] Permissions are minimal and explicit (no `id-token: write`)

## Nightly Soak (C1)
- [ ] Workflow exists: `.github/workflows/adr025-nightly-soak.yml`
- [ ] Triggers: `schedule` + `workflow_dispatch` only
- [ ] Artifact: `adr025-soak-report` retention 14 days

## Readiness Aggregation (C2)
- [ ] Workflow exists: `.github/workflows/adr025-nightly-readiness.yml`
- [ ] Triggers: `schedule` + `workflow_dispatch` only
- [ ] Aggregator script exists: `scripts/ci/adr025-soak-readiness-report.sh`
- [ ] Outputs: `nightly_readiness.json` + `nightly_readiness.md`
- [ ] Artifact: `adr025-nightly-readiness` retention 14 days

## Promotion criteria (hard-lock)
- [ ] Window definition is explicit (N runs or time window)
- [ ] Thresholds are explicit and measurable
- [ ] Classifier rules are deterministic and versioned (`classifier_version`)
- [ ] Explicit statement: "No PR required-check impact in Step3"

## Reviewer gates
- [ ] Reviewer script exists: `scripts/ci/review-adr025-i1-step3-c3.sh`
- [ ] Script enforces allowlist + policy checks
