# Wave56 Registry Trust/Cache Review Pack

Review intent:
- Convert `trust.rs` and `cache.rs` from large mixed facade/test files into thin public facades.
- Keep existing trust/cache runtime behavior unchanged.
- Move existing unit contract tests to the private `trust_next` and `cache_next` boundaries that already own the implementations.

Expected LOC after split:

| File | LOC |
| --- | ---: |
| `crates/assay-registry/src/trust.rs` | 184 |
| `crates/assay-registry/src/trust_next/tests.rs` | 415 |
| `crates/assay-registry/src/cache.rs` | 160 |
| `crates/assay-registry/src/cache_next/tests.rs` | 429 |

Primary review questions:
- Are the public `TrustStore`, `KeyMetadata`, `PackCache`, `CacheMeta`, and `CacheEntry` surfaces unchanged?
- Are the trust rotation, pinned-root, cache integrity, TTL, signature, and atomic-write tests moved rather than weakened?
- Are `trust_next::tests` and `cache_next::tests` compiled only in test builds?
- Did the PR avoid resolver, Cargo, workflow, runner, and eBPF drift?

Verification:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave56-registry-trust-cache.sh
```
