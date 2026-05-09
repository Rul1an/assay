# SPLIT REVIEW PACK - Wave 52 LiveKit Tool Action Step1

## Summary

Step1 aligns the LiveKit tool-action importer with LiveKit's source-level
`FunctionToolsExecutedEvent` behavior and freezes it before Wave52 module
splitting.

## Included

- list-order call/output pairing alignment
- optional `call_id` consistency checking
- `null` output preservation as `completed=false`
- schema, docs, and fixture updates
- Wave52 split plan, checklist, move map, and reviewer gate
- Week 8 SOTA gate plan and optional scripts
- OWASP MCP Top 10 test coverage map

## Excluded

- production module moves
- workflow changes
- Trust Basis classifier changes
- public family-matrix entry
- LiveKit endorsement claim

## Validation

Run:

```bash
bash scripts/ci/review-wave52-livekit-tool-action-step1.sh
```

The script checks:

- Step1 allowlist
- no module extraction yet
- `cargo fmt --check`
- `cargo check -p assay-cli`
- `cargo clippy -p assay-cli --all-targets -- -D warnings`
- `cargo test -q -p assay-cli livekit_tool_action`
- `cargo test -q -p assay-cli --test evidence_test test_livekit_imported_tool_action_receipts_verify_and_do_not_mutate_trust_basis_claims -- --exact`
- `bash scripts/ci/review-week8-sota-gates.sh`
- `git diff --check`

## Next Step

After Step1 lands, perform the mechanical split behind
`cmd_livekit_tool_action` and keep the receipt protocol contracts unchanged.
