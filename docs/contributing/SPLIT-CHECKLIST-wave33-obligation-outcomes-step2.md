# SPLIT CHECKLIST — Wave33 Obligation Outcomes Step2

## Scope discipline
- [ ] Only bounded runtime + tests + Step2 docs/gate files changed
- [ ] No `.github/workflows/*` changes
- [ ] No non-wave scope leaks

## Normalization contract
- [ ] `ObligationOutcome` carries additive normalization fields:
  - `reason_code`
  - `enforcement_stage`
  - `normalization_version`
- [ ] Existing compatibility fields remain:
  - `obligation_type`
  - `status`
  - `reason`
- [ ] Status semantics remain deterministic (`applied|skipped|error`)

## Reason-code baseline
- [ ] `legacy_warning_mapped` is emitted for legacy warning mapping
- [ ] `validated_in_handler` is emitted for handler-validated path
- [ ] `contract_only` is emitted for contract-only skips
- [ ] `unsupported_obligation_type` is emitted for unknown obligation types
- [ ] Existing approval/scope/redaction failure codes remain deterministic

## Behavior containment
- [ ] No change to allow/deny decision behavior
- [ ] No new obligation execution semantics introduced
- [ ] Existing `log`/`alert`/`approval_required`/`restrict_scope`/`redact_args` behavior stays stable

## Validation
- [ ] `BASE_REF=origin/codex/wave33-obligation-outcomes-step1-freeze bash scripts/ci/review-wave33-obligation-outcomes-step2.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests pass
