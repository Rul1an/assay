# Gateway Evidence Replay Gate-D Unlock

This note records the Gate-D state for `gateway-evidence-replay`.

## Current Decision

Gate-D is not unlocked by green CI alone. The merged MVP is internally sound, but product capture needs one more proof: a reviewer must be able to replay retained gateway-path evidence without trusting the gateway runtime and see a bounded verdict that refuses to overclaim.

Stage 1 is the unlock artifact: a digest-pinned internal demo over four retained-evidence bundles.

## Gates

1. **Perfect demo:** Green. The demo pack covers `path_verified`, `path_mismatch`, `incomplete`, and `invalid` over offline retained evidence.
2. **Incumbent replayability:** Held for every future product-facing step. Before Stage 2, re-check AGEF-style formats, replay-verdict-channel work, AEX, Auditable Agents, EBGP, BIV, and Vigil.
3. **Facts-only capture:** Not started. Assay must emit `gateway-path.v0` facts only; `gateway-evidence-replay` remains the verdict producer.
4. **Claim ceiling:** Held. No provider-honesty, response-truth, gateway-enforcement, TEE-root, safety, or compliance claim.

## Stage 2 Precondition

Do not build Assay capture until the demo review names a concrete producer and consumer for this evidence. A plausible future user is not enough.
