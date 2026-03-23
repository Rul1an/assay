# PLAN — T1b Trust Card MVP (2026q2)

> Status: Implemented on `main` (March 2026)
> Date: 2026-03-23
> Scope: deterministic render layer over [T1a](./PLAN-T1a-TRUST-BASIS-COMPILER-2026q2.md); [ADR-033](./ADR-033-OTel-Trust-Compiler-Positioning.md); [RFC-005](./RFC-005-trust-compiler-mvp-2026q2.md)

## 1) Goal

Ship `trustcard.json` (canonical) and `trustcard.md` (secondary) derived **only** from `generate_trust_basis` → `trust_basis_to_trust_card`. No second classification pass, no aggregate score, no badge semantics, no `trust_basis_sha256` in v1.

## 2) Frozen contract (v1)

| Item | Rule |
|------|------|
| `schema_version` | Always `1` for this wave (`TRUST_CARD_SCHEMA_VERSION`). |
| `claims[]` | `Vec<TrustBasisClaim>` — same serde as T1a; six frozen ids, same order as `TrustBasis::claims`. |
| `non_goals` | Three fixed strings in fixed order (`TRUST_CARD_NON_GOALS` in code); identical in JSON and Markdown. |
| Markdown `note` column | Empty T1a `note` renders as placeholder `-` (`TRUST_CARD_NOTE_EMPTY_PLACEHOLDER`); no multiline cells. |
| Markdown shape | Title + fixed five-column table + `## Non-goals` with literal bullets. |

## 3) Library API

Implementation: [`crates/assay-evidence/src/trust_card.rs`](../../crates/assay-evidence/src/trust_card.rs)

- `trust_basis_to_trust_card(&TrustBasis) -> TrustCard`
- `trust_card_to_canonical_json_bytes(&TrustCard) -> Result<Vec<u8>>` (pretty JSON + trailing newline, same pattern as trust basis)
- `trust_card_to_markdown(&TrustCard) -> String`

## 4) CLI

```bash
assay trustcard generate <BUNDLE> --out-dir <DIR> [--pack …] [--max-results N]
```

Writes `<DIR>/trustcard.json` and `<DIR>/trustcard.md`. No stdout artifact stream in v1; stderr may log paths.

## 5) Review gates

- **`trust_basis.rs`:** T1b changes only for shared type/export/serde glue if ever needed — not `classify_*` or claim ordering/content.
- Tests: `cargo test -p assay-evidence`, `cargo test -p assay-cli --test trustcard_test`, workspace `clippy` with `-D warnings`.

## 6) Explicit non-scope (this wave)

- `--from-trust-basis` / non-bundle input
- Trust card bytes on stdout
- Signing/attestation of card files
- `trust_basis_sha256` (or similar) in `trustcard.json` v1
