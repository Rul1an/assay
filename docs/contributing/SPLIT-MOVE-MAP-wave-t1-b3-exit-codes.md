# SPLIT MOVE MAP - Wave T1 B3 (contract_exit_codes)

## Section Move Map

- Root imports + shared helper functions kept in `contract_exit_codes.rs`.
- Core contract tests moved to `exit_codes/core.rs`:
  - CI/run argument/reason-code contracts
  - deprecation-deny contracts
- Replay/offline/bundle contracts moved to `exit_codes/replay.rs`.
- `test_status_map` helper remains in root file and reused by replay tests.

## Symbol Anchors

- Root helper anchors in `contract_exit_codes.rs`:
  - `read_run_json`
  - `read_summary_json`
  - `assert_schema`
  - `assert_run_json_seeds_early_exit`
  - `assert_run_json_seeds_happy`
  - `assert_summary_seeds_early_exit`
  - `assert_summary_seeds_happy`
  - `test_status_map`

## Test Anchors (unchanged)

- `contract_ci_report_io_failure`
- `contract_run_json_always_written_arg_conflict`
- `contract_reason_code_trace_not_found_v2`
- `contract_legacy_v1_trace_not_found`
- `contract_e72_seeds_happy_path`
- `contract_exit_codes_missing_config`
- `contract_replay_missing_dependency_offline`
- `contract_replay_verify_failure_writes_outputs_with_provenance`
- `contract_bundle_create_marks_missing_trace_as_incomplete_for_offline_replay`
- `contract_replay_roundtrip_from_created_bundle`
- `contract_replay_offline_is_hermetic_under_network_deny`
- `contract_run_deny_deprecations_fails_on_legacy_policy_usage`
- `contract_ci_deny_deprecations_fails_on_legacy_policy_usage`
