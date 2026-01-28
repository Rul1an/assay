# Implementation Plan: Implement Tool Signing (`x-assay-sig`)

This plan breaks down the work required to implement evidence bundle signing.

## Phase 1: Cryptography Foundation

- [ ] Task: Add the `ed25519-dalek` and `rand` crates as dependencies to `assay-evidence`.
- [ ] Task: Create a new module within `assay-evidence` for cryptographic operations (`crates/assay-evidence/src/crypto/signing.rs`).
- [ ] Task: Implement core signing and verification functions that operate on byte slices.
- [ ] Task: Write unit tests to ensure the cryptographic functions are correct (sign a message and verify it with the corresponding public key).
- [ ] Task: Conductor - User Manual Verification 'Cryptography Foundation' (Protocol in workflow.md)

## Phase 2: Implement `assay evidence sign` Command

- [ ] Task: Add the `sign` subcommand to `assay evidence` in the `assay-cli` crate's `clap` definition.
- [ ] Task: Implement the logic for the `sign` command:
    - [ ] Read the private key from the path specified by the `--key` flag.
    - [ ] Unpack the evidence bundle and parse its manifest.
    - [ ] Canonicalize the manifest JSON using the existing JCS implementation.
    - [ ] Sign the canonicalized bytes.
    - [ ] Add the signature to the manifest as the `x-assay-sig` field.
    - [ ] Re-pack the bundle with the new manifest.
- [ ] Task: Write integration tests for the `sign` command using a fixture keypair.
- [ ] Task: Conductor - User Manual Verification 'Implement `assay evidence sign` Command' (Protocol in workflow.md)

## Phase 3: Extend `assay evidence verify` Command

- [ ] Task: Add the `--pubkey` optional argument to the `verify` subcommand.
- [ ] Task: Modify the `verify` command's logic:
    - [ ] If `--pubkey` is present, proceed with signature verification.
    - [ ] Read the public key from the specified path.
    - [ ] Unpack the bundle and parse the manifest.
    - [ ] Extract the `x-assay-sig` field. If it's missing, fail with an error.
    - [ ] Canonicalize the manifest (making sure to exclude the `x-assay-sig` field itself from the canonicalization).
    - [ ] Verify the signature against the canonicalized data. If verification fails, return an error.
- [ ] Task: Update integration tests for `verify` to include successful and failed signature checks.
- [ ] Task: Conductor - User Manual Verification 'Extend `assay evidence verify` Command' (Protocol in workflow.md)

## Phase 4: Documentation

- [ ] Task: Update the CLI reference documentation in `docs/reference/cli/` for the `assay evidence sign` and `assay evidence verify` commands.
- [ ] Task: Add a new section to the guides (`docs/guides/`) explaining the evidence signing feature, its importance for supply chain security, and how to use it.
- [ ] Task: Conductor - User Manual Verification 'Documentation' (Protocol in workflow.md)
