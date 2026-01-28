# Specification: Implement Tool Signing (`x-assay-sig`)

## 1. Overview

This track implements Ed25519-based cryptographic signatures for evidence bundles to ensure authenticity and non-repudiation, as specified in ADR-011. This feature is a critical component of the supply chain security initiative.

## 2. Key Features

- **`x-assay-sig` Manifest Field:** A new, optional field `x-assay-sig` will be added to the evidence bundle manifest. It will contain the Ed25519 signature of the canonicalized manifest content.
- **Local Signing:** A new CLI command `assay evidence sign` will allow users to sign a bundle using a local private key.
- **Local Verification:** The existing `assay evidence verify` command will be extended to support verification against a provided public key.

## 3. Command-Line Interface (CLI)

### `assay evidence sign <BUNDLE_PATH> --key <PRIVATE_KEY_PATH>`
- Signs the specified `bundle.tar.gz`.
- The command will:
    1. Read the bundle and its manifest.
    2. Canonicalize the manifest JSON using JCS (RFC 8785).
    3. Generate an Ed25519 signature of the canonicalized data using the provided private key.
    4. Add the signature to the manifest under the `x-assay-sig` field.
    5. Re-package the bundle with the updated manifest.

### `assay evidence verify <BUNDLE_PATH> --pubkey <PUBLIC_KEY_PATH>`
- Extends the existing `verify` command.
- If the `--pubkey` flag is provided, the command will perform an additional verification step:
    1. Read the bundle and extract the manifest and the `x-assay-sig` signature.
    2. Canonicalize the manifest (excluding the signature field itself).
    3. Verify the signature against the canonicalized data using the provided public key.
    4. The command fails if the signature is invalid or missing when a public key is provided.

## 4. Non-Functional Requirements

- **Security:** Use a well-vetted Rust crate for Ed25519 implementation (e.g., `ed25519-dalek`). Private keys must be handled securely and never stored or logged.
- **Performance:** The signing and verification process should be fast, adding minimal overhead to the CLI commands.
- **Error Handling:** Provide clear error messages for invalid keys, missing signatures (when required), and signature verification failures.
- **Testability:** Include unit tests for the signing/verification logic and integration tests for the CLI commands.
