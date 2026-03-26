# Wave46 Step2 Move Map

Facade retained in `crates/assay-evidence/src/lint/packs/schema.rs`:

- module wiring for `schema_next`
- public re-exports
- existing inline tests

Moved to `crates/assay-evidence/src/lint/packs/schema_next/types.rs`:

- `PackKind`
- `PackDefinition`
- `PackRequirements`
- `PackRule`
- `CheckDefinition`
- `SupportedConditionalCheck`
- `SupportedConditionalClause`

Moved to `crates/assay-evidence/src/lint/packs/schema_next/serde.rs`:

- `serialize_pack_severity`
- `deserialize_pack_severity`

Moved to `crates/assay-evidence/src/lint/packs/schema_next/validation.rs`:

- `PackDefinition::validate`
- `PackRule::validate`
- `PackRule::canonical_id`
- `CheckDefinition::validate`
- `CheckDefinition::type_name`
- `CheckDefinition::get_field_paths`
- `is_valid_pack_name`

Moved to `crates/assay-evidence/src/lint/packs/schema_next/conditional.rs`:

- `RawConditionalCondition`
- `RawConditionalClause`
- `RawConditionalThen`
- `CheckDefinition::is_unsupported`
- `CheckDefinition::supported_conditional`

Moved to `crates/assay-evidence/src/lint/packs/schema_next/errors.rs`:

- `PackValidationError`
