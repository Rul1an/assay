# Agentic Step2 Move Map (Mechanical)

## Scope

- Source: `crates/assay-core/src/agentic/mod.rs`
- Destination root: `crates/assay-core/src/agentic/`

## Moves

| Old location | New location | Notes |
| --- | --- | --- |
| `build_suggestions` full body | `builder::build_suggestions_impl` | Pure move |
| policy/pointer helper functions | `policy_helpers.rs` | Pure move |
| private helper shape/cache types | `policy_helpers.rs` | Pure move |
| inline `#[cfg(test)] mod tests` | `tests/mod.rs` | Test names unchanged |
| `mod.rs` | facade-only public surface + wrapper | Behavioral equivalent |

## Facade contract

`mod.rs` retains:

- public types:
  - `RiskLevel`
  - `SuggestedAction`
  - `SuggestedPatch`
  - `JsonPatchOp`
  - `AgenticCtx`
- public entrypoint:
  - `build_suggestions` delegating to `builder::build_suggestions_impl`

No helper implementations remain in facade.
