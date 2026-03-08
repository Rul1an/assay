# Wave18 Step2 Move Map — `mandate/types.rs`

## Source

- `crates/assay-evidence/src/mandate/types.rs` (715 LOC)

## Target layout

- `crates/assay-evidence/src/mandate/types/mod.rs`
- `crates/assay-evidence/src/mandate/types/core.rs`
- `crates/assay-evidence/src/mandate/types/serde.rs`
- `crates/assay-evidence/src/mandate/types/schema.rs`
- `crates/assay-evidence/src/mandate/types/tests.rs`

## Mechanical mapping

- module docs + public exports -> `types/mod.rs`
- enums/structs/builders/impls (`MandateKind`, `OperationClass`, `Principal`, `Scope`, `Validity`, `Constraints`, `Context`, `Signature`, `Mandate`, `MandateBuilder`, `MandateContent`) -> `types/core.rs`
- `is_false` serde helper -> `types/serde.rs`
- payload type constants (`MANDATE_PAYLOAD_TYPE`, `MANDATE_USED_PAYLOAD_TYPE`, `MANDATE_REVOKED_PAYLOAD_TYPE`) -> `types/schema.rs`
- all existing unit tests (same names) -> `types/tests.rs`

## Notes

- `Constraints` serde attributes now reference `crate::mandate::types::serde::is_false`
- no behavior/API drift intended; this is 1:1 move/wiring only
