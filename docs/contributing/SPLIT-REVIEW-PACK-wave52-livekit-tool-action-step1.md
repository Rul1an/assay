# SPLIT REVIEW PACK - Wave 52 LiveKit Tool Action Step1

## Summary

Step1 starts Wave52 with a freeze-only slice for the LiveKit tool-action importer hotspot. The importer is now the largest handwritten production Rust file after Wave51, but it is also a fresh protocol-facing surface, so this PR intentionally adds no production changes.

## Included

- Wave52 split plan
- Step1 checklist
- Step1 move map
- Step1 review pack
- Step1 reviewer gate script
- Week 8 SOTA gate plan and optional scripts
- OWASP MCP Top 10 test coverage map

## Excluded

- production importer edits
- test edits
- schema/key allowlist changes
- hash/canonical JSON changes
- timestamp normalization changes
- Trust Basis behavior changes
- workflow changes

## Validation

Run:

```bash
bash scripts/ci/review-wave52-livekit-tool-action-step1.sh
```

The script checks:

- allowlist-only diff
- no workflow/generated-file changes
- no edits under LiveKit importer source or CLI tests
- `cargo fmt --check`
- `cargo check -p assay-cli`
- `cargo clippy -p assay-cli --all-targets -- -D warnings`
- `cargo test -q -p assay-cli livekit_tool_action`
- `cargo test -q -p assay-cli --test evidence_test test_livekit_imported_tool_action_receipts_verify_and_do_not_mutate_trust_basis_claims -- --exact`
- `bash scripts/ci/review-week8-sota-gates.sh`
- `git diff --check`

## Next Step

After Step1 lands, perform the mechanical split behind `cmd_livekit_tool_action` and keep the receipt protocol contracts unchanged.
