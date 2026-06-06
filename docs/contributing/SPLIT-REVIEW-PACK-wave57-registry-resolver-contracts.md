# Wave57 Registry Resolver Contracts Review Pack

Review intent:
- Freeze resolver behavior before the later `resolver_next` split.
- Increase confidence around the cache/registry decision boundary without changing runtime code.

Changed files expected:
- `crates/assay-registry/tests/resolver_contracts.rs`
- `docs/contributing/SPLIT-PLAN-wave57-registry-resolver.md`
- `docs/contributing/SPLIT-CHECKLIST-wave57-registry-resolver-contracts.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave57-registry-resolver-contracts.md`
- `scripts/ci/review-wave57-registry-resolver-contracts.sh`

Primary review questions:
- Do the tests assert externally observable resolver behavior rather than private implementation details?
- Are cache-first, pinned-digest refetch, and `no_cache` behavior all covered?
- Does the PR avoid edits to `resolver.rs` and other runtime code?
- Is the deferred 304 resolver behavior called out rather than accidentally normalized?

Verification:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave57-registry-resolver-contracts.sh
```
