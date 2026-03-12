# SPLIT MOVE MAP — Wave31 Redact Args Step2

## Intent
Implement the bounded `redact_args` contract/evidence shape from Wave31 Step1 without introducing runtime argument mutation.

## Code touch map
- `crates/assay-core/src/mcp/policy/mod.rs`
  - add typed `RedactArgsContract`
  - extend tool policy + obligation shape for `redact_args`
  - extend policy metadata for redaction contract/evidence fields
- `crates/assay-core/src/mcp/policy/engine.rs`
  - attach `redact_args` obligation for matched tools/classes
  - evaluate contract in contract-only mode
  - populate additive redaction evidence fields
- `crates/assay-core/src/mcp/tool_call_handler/emit.rs`
  - map redaction contract/evidence metadata into event context
- `crates/assay-core/src/mcp/decision.rs`
  - add Decision Event fields for redaction contract/evidence
  - thread fields through event/context builders
- `crates/assay-core/src/mcp/proxy.rs`
  - propagate redaction metadata into emitted decision events
- `crates/assay-core/src/mcp/obligations.rs`
  - mark `redact_args` as contract-only (`Skipped`) outcome

## Test touch map
- `crates/assay-core/src/mcp/tool_call_handler/tests.rs`
  - add `redact_args_contract_sets_additive_fields`
- `crates/assay-core/tests/decision_emit_invariant.rs`
  - add integration assertion for additive redaction fields
- `crates/assay-core/src/mcp/obligations.rs`
  - add `execute_log_only_marks_redact_args_as_contract_only`

## Out-of-scope confirmations
- no runtime args mutation/rewrite
- no `redact_args` deny semantics
- no broad/global scrub policies
- no PII/DLP integrations
