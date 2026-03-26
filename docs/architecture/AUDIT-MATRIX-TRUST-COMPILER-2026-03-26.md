# Trust Compiler Audit Matrix (2026-03-26)

This document is a compact audit matrix for the **trust-compiler line as it moved from MVP chain to first protocol product line** across the main delivery slice on **2026-03-24** and **2026-03-25**.

It is intentionally stricter than a narrative recap:

- **`main shipped?`** means merged on `origin/main`
- **`release-truth?`** distinguishes:
  - **`v3.3.0 released`** — included in tag/release/crates line `v3.3.0`
  - **`main only`** — merged on `main` after `v3.3.0`, not yet in a released tag
  - **`open PR only`** — proposed, not merged

The practical start of this slice is **`#923` / `#929`**, not the trust-compiler foundation PRs from **2026-03-23** (`#917` / `#918` / `#919` / `#920`), which established the north star and `T1a`.

## Matrix

| Wave / slice | PR(s) | Core files | Contract / result | `main` shipped? | Release-truth? |
|---|---|---|---|---|---|
| **T1b — Trust Card** | [#923](https://github.com/Rul1an/assay/pull/923) | `crates/assay-evidence/src/trust_card.rs`<br>`crates/assay-cli/src/cli/commands/trust_card.rs`<br>`docs/architecture/PLAN-T1b-TRUST-CARD-2026q2.md` | Deterministic `trustcard.json` / `trustcard.md` derived from Trust Basis only; no second semantic classification layer | **Yes** | **`v3.3.0` released** |
| **G3 — authorization context** | [#929](https://github.com/Rul1an/assay/pull/929) | `crates/assay-evidence/src/g3_authorization_context.rs`<br>`crates/assay-evidence/src/trust_basis.rs`<br>`docs/architecture/PLAN-G3-AUTHORIZATION-CONTEXT-EVIDENCE-2026q2.md` | MCP decision evidence gets bounded auth-context fields (`auth_scheme`, `auth_issuer`, `principal`); Trust Basis grows to **7 claims**; Trust Card JSON moves to **schema 2** with `authorization_context_visible` | **Yes** | **`v3.3.0` released** |
| **Outward positioning polish** | [#930](https://github.com/Rul1an/assay/pull/930) | `README.md`<br>`docs/community/DISCUSSIONS.md` | Public-facing trust-compiler wording sharpened; keeps claim-first positioning aligned with shipped surfaces | **Yes** | **`v3.3.0` released** |
| **P2a — MCP companion pack** | [#935](https://github.com/Rul1an/assay/pull/935) | `crates/assay-evidence/packs/mcp-signal-followup.yaml`<br>`packs/open/mcp-signal-followup/pack.yaml`<br>`docs/architecture/PLAN-P2a-MCP-SIGNAL-FOLLOWUP-CLAIM-PACK.md` | Built-in `mcp-signal-followup` pack (`MCP-001..003`); `MCP-001` shares G3 semantics with Trust Basis via `g3_authorization_context_present` | **Yes** | **`v3.3.0` released** |
| **Pack engine 1.2** | [#935](https://github.com/Rul1an/assay/pull/935) | `crates/assay-evidence/src/lint/packs/checks.rs`<br>`crates/assay-evidence/src/lint/packs/schema.rs`<br>`docs/architecture/SPEC-Pack-Engine-v1.md` | Pack engine **1.2** adds `g3_authorization_context_present`; MCP auth-context pack semantics become first-class in engine checks | **Yes** | **`v3.3.0` released** |
| **H1 — migration SSOT / alignment** | [#937](https://github.com/Rul1an/assay/pull/937) | `docs/architecture/MIGRATION-TRUST-COMPILER-3.2.md`<br>`docs/architecture/PLAN-H1-TRUST-KERNEL-ALIGNMENT-RELEASE-HARDENING.md`<br>`crates/assay-evidence/tests/h1_trust_kernel_alignment.rs` | Migration SSOT becomes the single truth for Trust Basis / Trust Card / pack floors / release floors; alignment tests and documentation harden the trust kernel | **Yes** | **`v3.3.0` released** |
| **H1a — shared vectors** | [#938](https://github.com/Rul1an/assay/pull/938) | `crates/assay-evidence/tests/common/` | Shared trust-kernel bundle vectors for H1 / P2a parity and future protocol-pack alignment | **Yes** | **`v3.3.0` released** |
| **Roadmap sync after H1** | [#939](https://github.com/Rul1an/assay/pull/939) | `docs/ROADMAP.md` | Marks H1 as shipped and sets **P2b** as next, preserving the ordered trust-compiler execution story | **Yes** | **`v3.3.0` released** |
| **P2b — A2A companion pack** | [#940](https://github.com/Rul1an/assay/pull/940) | `crates/assay-evidence/packs/a2a-signal-followup.yaml`<br>`packs/open/a2a-signal-followup/pack.yaml`<br>`docs/architecture/PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md` | Built-in `a2a-signal-followup` pack (`A2A-001..003`) on canonical `assay.adapter.a2a.*` evidence; presence-only discipline preserved | **Yes** | **`v3.3.0` released** |
| **`v3.3.0` release line** | release commit `1aa597c3` + tag [`v3.3.0`](https://github.com/Rul1an/assay/releases/tag/v3.3.0) | `CHANGELOG.md`<br>`docs/architecture/RELEASE-PLAN-TRUST-COMPILER-3.3.md` | First public release line bundling the trust-compiler baseline: Trust Basis, Trust Card schema 2 / 7 claims, G3, engine 1.2, built-in `mcp-signal-followup`, built-in `a2a-signal-followup`, migration SSOT | **Yes** | **`v3.3.0` released** |
| **Post-release hardening** | [#941](https://github.com/Rul1an/assay/pull/941) | `docs/ROADMAP.md`<br>`CHANGELOG.md`<br>`docs/architecture/RFC-005-trust-compiler-mvp-2026q2.md` | Post-`v3.3.0` docs hardening; keeps public docs and strategy docs aligned after the first trust-compiler release | **Yes** | **`main` only** |
| **G4 planning / freeze** | [#942](https://github.com/Rul1an/assay/pull/942), [#943](https://github.com/Rul1an/assay/pull/943) | `docs/architecture/PLAN-G4-A2A-DISCOVERY-CARD-EVIDENCE-2026q2.md` | G4 is frozen as an **evidence-wave**, not a pack-wave; discovery/card surfaces must be adapter-first and bounded before a follow-up pack exists | **Yes** | **`main` only** |
| **G4-A Phase 1** | [#944](https://github.com/Rul1an/assay/pull/944), [#945](https://github.com/Rul1an/assay/pull/945) | `docs/architecture/G4-A-PHASE1-FREEZE.md`<br>`crates/assay-adapter-a2a/` | A2A adapter emits first-class **`payload.discovery`** seam; Phase 1 is marked shipped and only post-merge verification / release-truth hygiene remains | **Yes** | **`main` only** |
| **P2c planning / lock** | [#946](https://github.com/Rul1an/assay/pull/946), [#947](https://github.com/Rul1an/assay/pull/947), [#948](https://github.com/Rul1an/assay/pull/948) | `docs/architecture/PLAN-P2c-A2A-DISCOVERY-CARD-FOLLOWUP-PACK.md` | P2c v1 frozen to **A2A-DC-001** / **A2A-DC-002**; decision lock includes **`value_equals`**, fail-default on pre-G4-A bundles, and **`requires >=3.3.0`** instead of substrate floor `>=3.2.3` | **Yes** | **`main` only** |
| **P2c — A2A discovery/card follow-up pack** | [#949](https://github.com/Rul1an/assay/pull/949) | `crates/assay-evidence/packs/a2a-discovery-card-followup.yaml`<br>`packs/open/a2a-discovery-card-followup/pack.yaml`<br>`crates/assay-evidence/src/lint/packs/checks.rs` | Built-in `a2a-discovery-card-followup`; first shipped use of `json_path_exists.value_equals` on frozen `payload.discovery` pointers; pack `requires.assay_min_version: ">=3.3.0"` | **Yes** | **`main` only** |
| **Post-P2c status sync** | commit `80975d78` | `CHANGELOG.md`<br>`docs/ROADMAP.md`<br>`docs/architecture/RFC-005-trust-compiler-mvp-2026q2.md` | Syncs `P2c` as shipped on `main` in roadmap / changelog / RFC-005; makes the current trust-compiler sequence auditable after the second A2A slice | **Yes** | **`main` only** |
| **Discovery next-wave note** | [#951](https://github.com/Rul1an/assay/pull/951) | `docs/architecture/DISCOVERY-NEXT-EVIDENCE-WAVE-2026Q2.md` | Discovery-only architecture note for what comes after `P2c`; frames candidate next evidence waves (`K1` handoff/delegation preferred) without changing roadmap or freezing a new wave | **No** | **open PR only** |

## Audit notes

- `T1a` and the trust-compiler north-star foundation (`#917` / `#918` / `#919` / `#920`) predate this two-day slice and are therefore treated as **prerequisite substrate**, not as part of the main matrix above.
- The **single source of truth** for trust-compiler version semantics remains [MIGRATION-TRUST-COMPILER-3.2.md](./MIGRATION-TRUST-COMPILER-3.2.md).
- The **first released trust-compiler baseline** is **`v3.3.0`**. `G4-A` and `P2c` are currently **merged on `main`** but **post-`v3.3.0`**, so they should not be described as released unless a later tag includes them.
- Consumers should key Trust Basis / Trust Card rows by stable **`claim.id`**, not by row count or ordering assumptions; “seven rows” is a **schema 2** fact, not a generic API contract.
