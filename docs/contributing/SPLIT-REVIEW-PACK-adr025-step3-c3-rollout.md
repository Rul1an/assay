# ADR-025 Step3 C3 Rollout Closure — Review Pack

## Intent
Close the Step3 loop by freezing rollout contracts:
- Informational nightly soak lane (C1)
- Informational readiness aggregation lane (C2)
- Explicit promotion criteria and classifier rules
- Reviewer gates to prevent PR blast radius and permission creep

## Non-goals
- No PR required-check changes
- No enforcement gate in PR lanes
- No release/promote fail-closed behavior (deferred to Step4)

## Hard contracts
### Trigger policy
- ADR-025 workflows must not include `pull_request`.
- Only `schedule` and `workflow_dispatch` are allowed.

### Permissions policy
- Explicit minimal permissions.
- No `id-token: write` outside explicit release/provenance flows.

### Supply-chain policy
- All actions must be SHA-pinned.

### Artifact contracts
- Soak artifact: `adr025-soak-report` (retention 14 days)
- Readiness artifact: `adr025-nightly-readiness` (retention 14 days)
- Readiness outputs: `nightly_readiness.json` + `nightly_readiness.md`

## Promotion criteria (informational in Step3)
Window (default):
- Last 20 soak runs (or a fixed time window if adopted later)

Thresholds (initial):
- contract_fail_rate (exit 2) <= 0.05
- infra_fail_rate (exit 3) <= 0.01
- success_rate (exit 0) >= 0.90
- unknown_rate <= 0.05

Classifier rules (deterministic):
- classifier_version: "1"
- Treat non-success workflow conclusions conservatively as contract failures unless refined in Step4.

## Verification
- Run: `bash scripts/ci/review-adr025-i1-step3-c3.sh`
- Local lint sanity:
  - `cargo fmt --check`
  - `cargo clippy -p assay-cli -- -D warnings`
  - `cargo test -p assay-cli`

## Reviewer checklist
- [ ] No PR triggers introduced
- [ ] SHA pinning enforced
- [ ] Minimal permissions enforced
- [ ] Promotion criteria are explicit and measurable
