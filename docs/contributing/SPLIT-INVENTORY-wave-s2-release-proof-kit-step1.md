# Wave S2 Inventory

## Goal

Ship a release proof kit that lets external consumers reproduce the existing
release provenance verification offline, using the exact same policy already
enforced by S1.

## In Scope

- `release.yml` proof-kit build step
- shared release archive inventory helper
- proof-kit builder script
- proof-kit contract tests
- release/proof-kit docs
- reviewer gate

## Out Of Scope

- new signing stack
- Rekor client
- runtime verifier
- registry/resolver work
- assay-action redesign
- generic DSSE/in-toto verifier
