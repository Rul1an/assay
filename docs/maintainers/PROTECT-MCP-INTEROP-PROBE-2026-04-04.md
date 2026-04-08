# Protect MCP Receipt Interop Probe

Date: 2026-04-04

## Summary

This probe was run as a bounded interop check, not as a product wave.

The current state is now clearer than in the first pass:

- the Assay-side repo-local probe is clean and deterministic
- existing MCP import regressions remain green
- the original external verifier mismatch was caused by a wrong corpus envelope
- the corrected Passport-envelope corpus now verifies under a pinned external boundary

This is a better result than the first pass, but it still does not justify opening a real evidence seam yet.

## Corpus used

Frozen fixture corpus:

- `tests/fixtures/interop/protect_mcp_receipts/issuer_key.json`
- `tests/fixtures/interop/protect_mcp_receipts/valid_allow.json`
- `tests/fixtures/interop/protect_mcp_receipts/valid_deny.json`
- `tests/fixtures/interop/protect_mcp_receipts/tampered.json`
- `tests/fixtures/interop/protect_mcp_receipts/malformed.json`

The valid and tampered fixtures now use the corrected Passport envelope shape:

- `payload`
- `signature`

The malformed fixture intentionally remains malformed.

## Repo-local probe result

Passed:

- `cargo test -q -p assay-core --test receipt_interop_probe`

Observed outcome:

- all fixtures load deterministically
- valid allow and valid deny receipts map to the bounded observed view
- malformed input is rejected as malformed by the local shape gate
- tampered input stays shape-valid but remains untrusted and unpromoted
- only `verification_result` is treated as theoretically elevatable in the probe harness
- binding is modeled as `issuer_id + session_id + sequence + tool_input_hash`

Regression safety also passed:

- `cargo test -q -p assay-core --test mcp_transport_compat`
- `cargo test -q -p assay-core --test mcp_id_correlation`
- `cargo test -q -p assay-cli --test mcp_transport_import`

## External verifier probe result

Verifier version tested:

- `npx @veritasacta/verify@0.2.4`

Manual boundary script:

- `bash tests/probes/protect_mcp_receipt_verifier_boundary.sh`

Result:

- passed against the corrected corpus and command contract

Observed behavior:

1. `valid_allow.json`
   - command: `npx @veritasacta/verify@0.2.4 valid_allow.json --key <public_key_hex>`
   - exit: `0`
   - stdout contains: `Signature: VALID`

2. `valid_deny.json`
   - command: `npx @veritasacta/verify@0.2.4 valid_deny.json --key <public_key_hex>`
   - exit: `0`
   - stdout contains: `Signature: VALID`

3. `tampered.json`
   - command: `npx @veritasacta/verify@0.2.4 tampered.json --key <public_key_hex>`
   - exit: `1`
   - stdout contains: `Signature: INVALID`

4. `malformed.json`
   - command: `npx @veritasacta/verify@0.2.4 malformed.json`
   - exit: `1`
   - stdout contains: `no_public_key`

Important correction from the discussion:

- the first corpus was wrong because it used root-level payload fields instead of Passport envelope shape
- the published verifier boundary that works here is the corrected one with `@0.2.4` plus explicit `--key` for the signed Passport-envelope receipts
- malformed input does not expose a separate exit code split; the current public CLI still advertises only exit codes `0` and `1`

## Unresolved contract gaps

These remain the blocking gaps before a real Assay seam would make sense:

- timestamp semantics stay signed-claim, not trusted fact
- `tool_input_hash` still needs a sharper interoperability contract if Assay ever wants to reason about it beyond opaque binding
- binding identity is really issuer plus session plus sequence plus input hash
- `verification_result` is only meaningful if Assay derives it or freezes a verifier boundary on purpose

Additional probe-specific caution after the rerun:

- the verifier boundary is better, but still young enough that Assay should pin version and invocation shape explicitly if this ever moves beyond a probe
- the current successful path depends on explicit `--key` usage for the signed Passport-envelope receipts

## Current maintainer call

Do not open a product wave from this yet.

The correct next step, if the discussion continues, is still a tighter contract note on:

- exact signature payload and canonicalization rules
- exact verifier contract and versioned behavior
- exact binding identity and collision assumptions
- the difference between signed claims and Assay-promotable facts

See also:

- `docs/maintainers/PROTECT-MCP-RECEIPT-CONTRACT-NOTE-2026-04-04.md`
- `docs/maintainers/PROTECT-MCP-RECEIPT-DECISION-2026-04-04.md`
