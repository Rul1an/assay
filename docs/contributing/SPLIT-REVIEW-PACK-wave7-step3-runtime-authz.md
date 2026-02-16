# Review Pack: Wave7 Step3 (runtime authz closure)

Intent:
- Finalize runtime authz closure with thin, testless facade.
- Keep public API/signatures and behavior contracts stable.

Scope:
- `crates/assay-core/src/runtime/authorizer.rs`
- `crates/assay-core/src/runtime/authorizer_internal/`
- `crates/assay-core/src/runtime/mandate_store.rs` (txn boundary continuity checks)
- `crates/assay-core/src/runtime/mandate_store_next/` (txn single-source checks)
- `docs/contributing/SPLIT-CHECKLIST-wave7-step3-runtime-authz.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave7-step3-runtime-authz.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave7-step3-runtime-authz.md`
- `scripts/ci/review-wave7-step3.sh`
- `docs/architecture/PLAN-split-refactor-2026q1.md`

## 1) Facade closure proof (`authorizer.rs`)

Expected:
- no `#[cfg(test)]`
- no `mod tests`
- public entrypoints delegate to:
  - `authorizer_internal::run::authorize_and_consume_impl`
  - `authorizer_internal::run::authorize_at_impl`

## 2) Boundary single-source proof

- Orchestration calls (`policy::check_*`, `store::check_*`) only in `authorizer_internal/run.rs`.
- Store mutation/read calls (`get_revoked_at`, `upsert_mandate`, `consume_mandate`) only in `authorizer_internal/store.rs`.
- Transaction-ref hash helper (`serde_jcs`, `sha2`, `hex::encode`) only in `authorizer_internal/policy.rs` and `authorizer_internal/tests.rs`.
- Mandate txn SQL boundary (`BEGIN|COMMIT|ROLLBACK`) only in `mandate_store_next/txn.rs`.

## 3) Contract anchors retained

Anchor tests (same names):
- `test_authorize_rejects_expired`
- `test_authorize_rejects_not_yet_valid`
- `test_authorize_rejects_tool_not_in_scope`
- `test_authorize_rejects_transaction_ref_mismatch`
- `test_authorize_rejects_revoked_mandate`
- `test_multicall_produces_monotonic_counts_no_gaps`
- `test_multicall_idempotent_same_tool_call_id`
- `test_revocation_roundtrip`
- `test_compute_use_id_contract_vector`
- `test_two_connections_same_tool_call_id_has_single_new_receipt`

Validation:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave7-step3.sh
```
