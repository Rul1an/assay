# SPLIT CHECKLIST - Wave41 Consumer Hardening Step2

## Scope discipline
- [ ] Diff is limited to bounded consumer-hardening runtime + tests + Step2 docs/gate.
- [ ] No `.github/workflows/*` changes.
- [ ] No scope leaks outside decision/replay consumer-compat paths.
- [ ] No runtime behavior change is introduced.
- [ ] No new policy-engine/control-plane/auth transport scope is added.

## Implementation contract
- [ ] Additive consumer-hardening fields are present:
  - `decision_consumer_contract_version`
  - `consumer_read_path`
  - `consumer_fallback_applied`
  - `consumer_payload_state`
  - `required_consumer_fields`
- [ ] Deterministic consumer precedence is represented and test-covered:
  - converged decision fields
  - compatibility markers
  - legacy decision fallback
- [ ] Consumer-facing fallback metadata remains additive and backward-compatible.

## Compatibility and behavior
- [ ] Existing decision/event fields remain backward-compatible.
- [ ] Existing runtime decision behavior remains unchanged.
- [ ] Replay/diff consumers can read payloads deterministically without bespoke precedence logic.

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave41-consumer-hardening-step2.sh` passes.
- [ ] `cargo fmt --check` passes.
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes.
- [ ] Pinned runtime/event tests pass.
