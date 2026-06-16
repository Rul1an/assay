# Rekor v2 offline-inclusion test vectors (MCP04a-3.3c)

Independent, upstream vectors for the a-3.3c Rekor v2 inclusion verifier (built next).

- Source: `github.com/sigstore/sigstore-conformance`, `test/assets/bundle-verify/`
- Pinned commit: `3d8491f6a3a54b1a7bb5c28ee2029fbe3ff521e3`
- License: Apache-2.0 (upstream)
- Shape: Rekor v2 `hashedrekord/0.0.2`, checkpoint-only (no SET). `canonicalizedBody` is canonical JSON.
- Pinned key: the trusted_root `tlogs[]` Ed25519 (`PKIX_ED25519`) entry whose log id matches the
  checkpoint origin; the ECDSA P-256 tlog is the old v1 log.

Expected (dir name `_fail` => must NOT verify): `rekor2-happy-path` / `rekor2-dsse-happy-path` verify;
`rekor2-no-inclusion-proof_fail` (missing proof), `rekor2-checkpoint-no-matching-signature_fail`,
`rekor2-checkpoint-missing-root-hash_fail`, `rekor2-dsse-mismatch-sig_fail` fail.
`v1-hashedrekord` is a Rekor v1 bundle (hashedrekord 0.0.1 + SET) for the `UnsupportedFormat` path.
The D-LEAF=B leaf-binding negative is programmatic (the test mutates `rekor2-happy-path`'s leaf cert so
the entry's `canonicalizedBody` no longer matches the bundle).

Full package + per-file digests: experiments `MCP04a3-3c-rekor-vectors/MANIFEST.md`.
