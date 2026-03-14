# SPLIT CHECKLIST - Wave42 Context Envelope Step2

## Scope discipline
- [ ] Diff is limited to bounded context-envelope runtime + tests + Step2 docs/gate.
- [ ] No `.github/workflows/*` changes.
- [ ] No scope leaks outside decision/context-envelope payload paths.
- [ ] No runtime behavior change is introduced.
- [ ] No new policy-engine/control-plane/auth transport scope is added.

## Implementation contract
- [ ] Additive context-envelope fields are present:
  - `decision_context_contract_version`
  - `context_payload_state`
  - `required_context_fields`
  - `missing_context_fields`
- [ ] Deterministic completeness semantics are represented and test-covered:
  - complete envelope
  - partial envelope
  - absent envelope
- [ ] Context-facing metadata remains additive and backward-compatible.

## Compatibility and behavior
- [ ] Existing decision/event fields remain backward-compatible.
- [ ] Existing runtime decision behavior remains unchanged.
- [ ] Downstream readers can reason about envelope completeness deterministically.

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave42-context-envelope-step2.sh` passes.
- [ ] `cargo fmt --check` passes.
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes.
- [ ] Pinned runtime/event tests pass.
