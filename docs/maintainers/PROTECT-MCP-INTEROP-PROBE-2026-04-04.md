# Protect MCP Receipt Interop Probe

Date: 2026-04-04

## Summary

This probe was run as a bounded interop check, not as a product wave.

The current state is mixed:

- the Assay-side repo-local probe is clean and deterministic
- existing MCP import regressions remain green
- the advertised external verifier contract does not currently match reality

The result is useful, but it does not justify opening a real evidence seam yet.

## Corpus used

Frozen fixture corpus:

- `tests/fixtures/interop/protect_mcp_receipts/issuer_key.json`
- `tests/fixtures/interop/protect_mcp_receipts/valid_allow.json`
- `tests/fixtures/interop/protect_mcp_receipts/valid_deny.json`
- `tests/fixtures/interop/protect_mcp_receipts/tampered.json`
- `tests/fixtures/interop/protect_mcp_receipts/malformed.json`

The receipt payloads were copied literally from the GitHub discussion corpus shared by `tomjwxf`, including the real Ed25519 signatures and the issuer public key.

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

- `npx @veritasacta/verify@0.2.2`

Manual boundary script:

- `bash tests/probes/protect_mcp_receipt_verifier_boundary.sh`

Result:

- failed with 4 contract drift issues

Observed drift against the discussion-stated contract:

1. `valid_allow.json`
   - expected: exit `0`
   - actual: exit `1`
   - stdout: `no_public_key`

2. `valid_deny.json`
   - expected: exit `0`
   - actual: exit `1`
   - stdout: `no_public_key`

3. `tampered.json`
   - expected: exit `1` with `FAILED:`
   - actual: exit `1`, but not in the claimed failure shape
   - stdout: `no_public_key`

4. `malformed.json`
   - expected: exit `2` with parse or malformed signal on `stderr`
   - actual: exit `1`
   - stdout: `no_public_key`

Additional exploratory follow-up with the provided public key:

- `npx @veritasacta/verify@0.2.2 --key <public_key_hex> valid_allow.json`
  - exit `1`
  - stdout: `verification_error`

- `npx @veritasacta/verify@0.2.2 --key <public_key_hex> valid_deny.json`
  - exit `1`
  - stdout: `verification_error`

- `npx @veritasacta/verify@0.2.2 --key <public_key_hex> tampered.json`
  - exit `1`
  - stdout: `verification_error`

- `npx @veritasacta/verify@0.2.2 --key <public_key_hex> malformed.json`
  - exit `1`
  - stdout: `missing_signature`

The verifier help output also diverges from the stated discussion contract:

- help advertises only exit codes `0` and `1`
- help does not advertise a dedicated exit code `2` path for malformed input

## Unresolved contract gaps

These remain the blocking gaps before a real Assay seam would make sense:

- timestamp semantics stay signed-claim, not trusted fact
- `tool_input_hash` still needs a canonicalization contract
- binding identity is really issuer plus session plus sequence plus input hash
- `verification_result` is only meaningful if Assay derives it or freezes a verifier boundary on purpose

Additional probe-specific concern after the real run:

- the current external verifier contract is not yet stable enough to treat as a clean boundary
- even with the supplied public key, the "valid" sample receipts do not currently verify under `@veritasacta/verify@0.2.2`

## Current maintainer call

Do not open a product wave from this yet.

The correct next step, if the discussion continues, is a tighter contract note on:

- exact signature payload and canonicalization rules
- exact verifier contract and versioned behavior
- exact binding identity and collision assumptions
- the difference between signed claims and Assay-promotable facts
