# SPLIT CHECKLIST - ADR-026 Stabilization E4 (closure)

## Scope
- [ ] Only E4 closure docs and reviewer gate changed
- [ ] No `.github/workflows/*` changes
- [ ] No adapter runtime behavior changes in this slice

## Hardening chain present
- [ ] E0 adapter metadata contract is present in docs and code
- [ ] E1 ACP lossiness preservation is implemented and gated
- [ ] E2 host `AttachmentWriter` boundary is implemented and gated
- [ ] E3 canonical payload digest contract is implemented and gated
- [ ] E4 parser caps and property tests are implemented and gated

## Parser hardening invariants
- [ ] `max_payload_bytes` remains enforced at adapter ingress
- [ ] `max_json_depth` is enforced for ACP and A2A payloads
- [ ] `max_array_length` is enforced for ACP and A2A payloads
- [ ] Invalid UTF-8 and malformed JSON fail with measurement errors in all modes
- [ ] Lenient mode does not bypass parser/cap failures
- [ ] Property/proptest coverage exists for ACP and A2A unknown-field lossiness accounting

## Reviewer gate
- [ ] `scripts/ci/review-adr026-stab-e4-c.sh` exists
- [ ] Gate enforces allowlist-only + workflow-ban
- [ ] Gate re-runs the E4B implementation gate against `origin/main`
