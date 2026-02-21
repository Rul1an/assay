# ADR-025 I3 Step3 Checklist (OTel bridge rollout informational)

## Scope
- [ ] Workflow exists: `.github/workflows/adr025-nightly-otel-bridge.yml`
- [ ] Reviewer gate exists: `scripts/ci/review-adr025-i3-step3.sh`
- [ ] Review pack exists: `docs/contributing/SPLIT-REVIEW-PACK-adr025-i3-step3.md`

## Trigger/permissions contracts
- [ ] Triggers are `schedule` + `workflow_dispatch` only
- [ ] No `pull_request`/`pull_request_target`
- [ ] Job-level `continue-on-error: true`
- [ ] Actions are SHA-pinned
- [ ] Minimal permissions (`contents: read`, `actions: write`)

## Artifact contract
- [ ] Upload artifact `adr025-otel-bridge-report`
- [ ] Contains `otel_bridge_report_v1.json` and `otel_bridge_report_v1.md`
- [ ] Retention is 14 days

## Stabilization coverage (I3 Stab B)
- [ ] Multi-trace ordering covered by fixture tests
- [ ] Multi-span ordering + parent ID lowercase normalization covered
- [ ] Attribute/event/link sorting invariants covered
- [ ] unix_nano int vs digit-string inputs normalize to digit-strings
- [ ] Contract failures still map to exit code `2`

## Safety
- [ ] Informational-only lane (no PR required-check impact)
- [ ] No changes outside Step3 allowlist
