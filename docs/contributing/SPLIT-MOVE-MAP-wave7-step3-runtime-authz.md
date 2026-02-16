# Wave7 Step3 move-map (runtime authz closure)

Closure decisions:
- Internal naming finalized as `authorizer_internal/*` (no temporary `*_next` naming left for authorizer).
- `authorizer.rs` stays the stable public facade file.
- Authorizer tests moved to `authorizer_internal/tests.rs`; facade has no test module.

Module responsibilities:
- `authorizer.rs`: public types + public function delegation only.
- `authorizer_internal/run.rs`: orchestration only.
- `authorizer_internal/policy.rs`: policy evaluation + glob matching + transaction-ref hash helper.
- `authorizer_internal/store.rs`: MandateStore interaction boundary only.
- `authorizer_internal/tests.rs`: authorizer unit/contract tests.

Function -> file map:
- `Authorizer::authorize_and_consume` -> `authorizer_internal/run.rs::authorize_and_consume_impl`
- `Authorizer::authorize_at` -> `authorizer_internal/run.rs::authorize_at_impl`
- `check_validity_window_impl` -> `authorizer_internal/policy.rs`
- `check_context_impl` -> `authorizer_internal/policy.rs`
- `check_scope_impl` -> `authorizer_internal/policy.rs`
- `check_operation_class_impl` -> `authorizer_internal/policy.rs`
- `check_transaction_ref_impl` -> `authorizer_internal/policy.rs`
- `tool_matches_scope_impl` -> `authorizer_internal/policy.rs`
- `glob_matches_impl` -> `authorizer_internal/policy.rs`
- `compute_transaction_ref_impl` -> `authorizer_internal/policy.rs`
- `check_revocation_impl` -> `authorizer_internal/store.rs`
- `upsert_mandate_metadata_impl` -> `authorizer_internal/store.rs`
- `consume_mandate_impl` -> `authorizer_internal/store.rs`

Caller chains (entrypoint level):
1. `Authorizer::authorize_and_consume`
   -> `authorizer_internal::run::authorize_and_consume_impl`
   -> `authorizer_internal::run::authorize_at_impl`
   -> policy/store helper chain.

2. `Authorizer::authorize_at`
   -> `authorizer_internal::run::authorize_at_impl`
   -> `policy::check_*` + `store::check_*` calls
   -> returns `AuthzReceipt`.

3. Scope + tx-ref paths:
   - scope: `policy::check_scope_impl` -> `policy::tool_matches_scope_impl` -> `policy::glob_matches_impl`
   - tx-ref: `policy::check_transaction_ref_impl` -> `policy::compute_transaction_ref_impl`

Mechanical contract:
- No public signature changes.
- No contract test semantic changes.
- No error wording changes.
