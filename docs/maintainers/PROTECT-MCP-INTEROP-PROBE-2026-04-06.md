# Protect MCP Receipt Interop Probe Follow-Up

Date: 2026-04-06

## Summary

This follow-up reran the external verifier boundary after Tom shipped `@veritasacta/verify@0.2.5` with the promised 3-way exit code split.

The result is materially better than the `@0.2.4` state:

- the cleaner verifier classification now holds on our side
- the repo-local Assay probe still stays clean and bounded
- the current maintainer call does not change: this remains a probe, not a product seam

## What changed

The pinned manual boundary script now targets:

- `npx @veritasacta/verify@0.2.5`

The current observed exit-code contract on our side is:

- `0` = valid signature
- `1` = invalid signature
- `2` = verifier error

This is the split we had asked for in the discussion because it keeps genuine tamper failures separate from "the verifier could not make a determination."

## Repo-local probe result

Passed:

- `cargo test -q -p assay-core --test receipt_interop_probe`
- `cargo test -q -p assay-core --test mcp_transport_compat`
- `cargo test -q -p assay-core --test mcp_id_correlation`
- `cargo test -q -p assay-cli --test mcp_transport_import`

Observed outcome:

- the bounded Assay-side interpretation did not need to widen
- valid and tampered samples still behave the same in the local harness
- existing MCP import seams remain unaffected

## External verifier probe result

Passed:

- `bash tests/probes/protect_mcp_receipt_verifier_boundary.sh`

Observed behavior with `@veritasacta/verify@0.2.5`:

1. `valid_allow.json`
   - exit: `0`
   - stdout contains: `Signature: VALID`

2. `valid_deny.json`
   - exit: `0`
   - stdout contains: `Signature: VALID`

3. `tampered.json`
   - exit: `1`
   - stdout contains: `Signature: INVALID`

4. `malformed.json`
   - exit: `2`
   - stdout contains: `no_public_key`

Important nuance:

- the exit-code split is now much cleaner
- the human-readable stream output is still not something Assay should treat as a semantic contract
- the stable boundary worth freezing, if this ever graduates, is version plus invocation shape plus exit-code behavior

## Maintainer call

This improves confidence in the verifier boundary, but it still does not justify opening an Assay wave.

Current call remains:

- keep this at probe status
- do not imply adoption
- do not freeze public Assay schema from it yet

The verifier side is now in better shape. The remaining caution is still around contract scope:

- timestamp semantics remain claim-shaped
- `tool_input_hash` remains an opaque binding token, not a hard fact
- binding identity assumptions still need deliberate freezing if this ever moves beyond probe status

See also:

- `docs/maintainers/PROTECT-MCP-INTEROP-PROBE-2026-04-04.md`
- `docs/maintainers/PROTECT-MCP-RECEIPT-CONTRACT-NOTE-2026-04-04.md`
- `docs/maintainers/PROTECT-MCP-RECEIPT-DECISION-2026-04-04.md`
