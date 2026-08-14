# Outward Product Truth Slice 2 — Completed Record

This completed work corrected the public description of the deterministic
`run_root` digest without changing the evidence wire format or runtime
behavior.

## Implemented contract

`run_root` is SHA-256 over newline-delimited event content-hash strings, with
a trailing newline, in event sequence order. Verification recomputes whether
carried bytes match the recorded manifest and that deterministic digest; it
does not establish a provider outcome or an external side effect.

## Guard scope

The vocabulary guard reads every tracked textual file except its two
implementation paths. Publication configuration may keep this directory out
of the rendered documentation site, but it does not remove these files from
repository-visible scanning. A future plan containing a false digest claim
must fail the guard.

The guard uses path-bound complete-line permits for reviewed, independent
tree-based constructions and generated identifiers. Historical ADR, RFC, and
experiment text retains its original record only alongside a dated correction;
frozen generated artifacts use their pinned correction sidecar.

## Experiment output

The E3 cost artifact's `synthetic_log2_hash_count` is a comparison model only.
Production verification does not consume it. Both the full sweep and the
focused output test use the same row constructor, so the focused test detects
a regression in the emitted key without running the full sweep.
