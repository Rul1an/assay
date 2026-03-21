# Release Proof Kit

Assay release builds now ship a proof kit alongside the release archives. The kit
is the canonical consumer verification path for release provenance.

## What The Kit Contains

- `manifest.json`
- `release-provenance.json`
- `release-provenance.json.sha256`
- `trusted_root.jsonl`
- `bundles/*.jsonl`
- `verify-offline.sh`
- `verify-release-online.sh`
- `README.md`

## Canonical Verification Path

The canonical path is offline verification with the shipped kit:

```bash
tar -xzf assay-vX.Y.Z-release-proof-kit.tar.gz
cd release-proof-kit
./verify-offline.sh --assets-dir /path/to/release-assets
```

`verify-offline.sh` is a thin wrapper around `gh attestation verify`. It does
not invent local policy, does not fall back to online verification, and fails
closed if the manifest, trusted root snapshot, bundles, or release artifacts are
missing.

## Convenience Online Cross-Check

`verify-release-online.sh` is convenience-only. It wraps:

- `gh release verify`
- `gh release verify-asset`

This helper is not the canonical truth path for the kit.

## Policy Source

The proof-kit manifest is derived from the `verification_policy` already emitted
by `release-provenance.json`. In particular, it reuses:

- `repo`
- `signer_workflow`
- `cert_oidc_issuer`
- `source_ref`
- `source_digest`
- `predicate_type`
- `deny_self_hosted_runners`

The kit does not define a second provenance policy.

## Trusted Root Snapshot

`trusted_root.jsonl` is stored in the exact format emitted by `gh attestation trusted-root`.

`trusted_root_generated_at` records when the snapshot was captured for this kit.
It does not imply permanent validity, revocation awareness, or a generic
root-of-trust timestamp outside the GitHub artifact-attestation model.

Refresh the trusted-root snapshot whenever you import newer signed material into
an offline environment.

## Non-Goals

The release proof kit does not provide:

- general Sigstore verification
- generic Rekor verification
- a complete supply-chain guarantee
- runtime trust enforcement
- a proof system outside the GitHub artifact-attestation model
