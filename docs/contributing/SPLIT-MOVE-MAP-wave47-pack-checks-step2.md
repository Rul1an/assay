# Wave47 Step2 Move Map

Facade retained in `crates/assay-evidence/src/lint/packs/checks.rs`:

- `CheckContext`
- `CheckResult`
- `ENGINE_VERSION`
- `execute_check`
- engine-version gate + unsupported-check handling
- `#[path = "checks_next/mod.rs"] mod checks_next;`
- existing inline tests

Moved to `crates/assay-evidence/src/lint/packs/checks_next/event.rs`:

- `check_g3_authorization_context_present`
- `check_event_count`
- `check_event_pairs`
- `check_event_field_present`
- `check_event_type_exists`
- `compile_glob`
- `scoped_events`

Moved to `crates/assay-evidence/src/lint/packs/checks_next/json_path.rs`:

- `check_json_path_exists`
- `value_pointer`

Moved to `crates/assay-evidence/src/lint/packs/checks_next/conditional.rs`:

- `check_conditional`

Moved to `crates/assay-evidence/src/lint/packs/checks_next/manifest.rs`:

- `check_manifest_field`

Moved to `crates/assay-evidence/src/lint/packs/checks_next/finding.rs`:

- `create_finding`
- `create_finding_with_severity`
- `event_location`
- `LintFindingExt`

Explicitly unchanged in this wave:

- `crates/assay-evidence/src/lint/packs/schema.rs`
- `crates/assay-evidence/src/lint/packs/schema_next/**`
- `crates/assay-evidence/tests/**`
- `packs/open/**`
