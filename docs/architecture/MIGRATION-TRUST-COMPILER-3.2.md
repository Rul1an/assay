# Migration & release truth — Trust Compiler 3.2 line

**Canonical document (use this path everywhere):** [`MIGRATION-TRUST-COMPILER-3.2.md`](MIGRATION-TRUST-COMPILER-3.2.md) — do not introduce parallel migration filenames for the same contract line.

**Single source of truth (SSOT)** for Trust Basis, Trust Card, pack engine, and companion-pack contract floors (`mcp-signal-followup`, `a2a-signal-followup`, **`a2a-discovery-card-followup` / P2c**). Other docs (CHANGELOG, README, [PLAN-P2a](PLAN-P2a-MCP-SIGNAL-FOLLOWUP-CLAIM-PACK.md), [PLAN-P2b](PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md), [PLAN-P2c](PLAN-P2c-A2A-DISCOVERY-CARD-FOLLOWUP-PACK.md)) point here instead of duplicating version semantics.

For the hardening wave that introduced this document, see [PLAN-H1 — Trust Kernel Alignment & Release Hardening](PLAN-H1-TRUST-KERNEL-ALIGNMENT-RELEASE-HARDENING.md).

## Two-layer version truth (substrate floor vs embedded packs)

- **`requires.assay_min_version: ">=3.2.3"`** on companion packs is the **evidence-substrate floor** (G3 + Trust Card schema 2 + seven claims). The **v3.2.3** tag is the usual reference for that prerequisite line — it does **not** imply that every built-in companion pack was already embedded in the CLI.
- **First release embedding both** built-in companion packs (`mcp-signal-followup` **and** `a2a-signal-followup`) in published **assay** binaries is **v3.3.0** — see [CHANGELOG.md](../../CHANGELOG.md) § 3.3.0. Do not read substrate tags (e.g. v3.2.3) as “both packs were already in the binary.”
- **P2c** (`a2a-discovery-card-followup`) uses a **different `requires` meaning**: **`>=3.3.0`** is the **G4-A-capable** workspace line (adapter emits `payload.discovery`), **not** the P2a/P2b evidence-substrate `>=3.2.3` floor — see [§ `a2a-discovery-card-followup`](#a2a-discovery-card-followup-built-in-pack-p2c) below.

## Consumer contract (non-negotiable)

**Integrations must key trust claims by `claim.id`, not by table position, row index, or implicit row count.** Order and count can change when `schema_version` changes; stable `id` is the only portable selector. Treat “seven rows” or “row N” as documentation hints for **schema_version = 2** only, not API contracts.

## Trust Card invariants (mechanical)

- **Top-level JSON keys** stay limited to the frozen surface: `schema_version`, `claims`, `non_goals` — no parallel claim model or extra semantic layers.
- For a given **`schema_version`**, **claim order, count, and id-set** match `generate_trust_basis` for that schema; the card **does not** reclassify or filter claims. A future schema version **may** change count and/or order — consumers still key by `id` only.
- **Rendering** (`trust_basis_to_trust_card`, markdown table) **adds no claim classification** beyond copying `TrustBasis.claims` and attaching frozen non-goals text.

| Field | Value |
|-------|--------|
| `schema_version` | **2** (adds G3 `authorization_context_visible` in the same row model as v1) |
| Claim rows | **7** `TrustBasisClaim` entries when `schema_version` is **2** (Trust Compiler 3.2 line); future versions may use a different count. |
| Semantics | Copy-only from Trust Basis + frozen `non_goals` (see invariants above). |

## Pack engine (evidence lint)

| Item | Value |
|------|--------|
| `ENGINE_VERSION` | **1.2** (`crates/assay-evidence/src/lint/packs/checks.rs`) |
| New check type | `g3_authorization_context_present` (same G3 v1 predicate as Trust Basis `authorization_context_visible` when verified) |
| `json_path_exists` | Optional **`value_equals`** (JSON equality, no coercion) for P2c boolean `true` checks — **no** `ENGINE_VERSION` bump |

## `mcp-signal-followup` built-in pack

- **Name / version:** `mcp-signal-followup` @ `1.0.0`
- **`requires.assay_min_version`:** `>=3.2.3` tracks the **first released Assay line** with G3 + Trust Card schema prerequisites on the evidence substrate (**v3.2.3** is the reference tag for that prerequisite availability, not necessarily the first binary that embeds the built-in pack).
- **Built-in pack + engine 1.2** ship with the Assay **release that contains P2a**; confirm the **first published version/tag** that embeds `mcp-signal-followup` in release notes.

Details and options (bump floor vs document-only): [PLAN-P2a](PLAN-P2a-MCP-SIGNAL-FOLLOWUP-CLAIM-PACK.md) § `assay_min_version`.

## `a2a-signal-followup` built-in pack (P2b)

- **Name / version:** `a2a-signal-followup` @ `1.0.0`
- **Authoritative YAML:** `crates/assay-evidence/packs/a2a-signal-followup.yaml` — `requires.assay_min_version: ">=3.2.3"` (and `evidence_schema_version: "1.0"`). Same **meaning** as P2a: the floor tracks the **evidence substrate** line (G3 + Trust Card schema 2 + seven claims; **v3.2.3** reference tag), **not** automatically the first GitHub/crates.io release that embeds this built-in pack — state the latter in release notes ([PLAN-P2b](PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md) § `assay_min_version`).
- **Rules:** A2A-001..003 — `event_type_exists` on canonical `assay.adapter.a2a.*` types; **no** G3 predicate; **no** `ENGINE_VERSION` bump for P2b.

## `a2a-discovery-card-followup` built-in pack (P2c)

- **Name / version:** `a2a-discovery-card-followup` @ `1.0.0`
- **Authoritative YAML:** `crates/assay-evidence/packs/a2a-discovery-card-followup.yaml` — `requires.assay_min_version: ">=3.3.0"` and `evidence_schema_version: "1.0"`. The floor tracks the **first Assay line that ships G4-A `payload.discovery`** in the adapter + evidence consumers expect, **not** the P2a/P2b `>=3.2.3` substrate story — see [PLAN-P2c](PLAN-P2c-A2A-DISCOVERY-CARD-FOLLOWUP-PACK.md), [G4-A-PHASE1-FREEZE.md](G4-A-PHASE1-FREEZE.md).
- **Rules:** A2A-DC-001 / A2A-DC-002 — `json_path_exists` with **`value_equals: true`** on frozen `/data/discovery/*` pointers (boolean **JSON** `true` only).
- **First published binary** that embeds this built-in: state explicitly in release notes for the tag that first ships it (code may land on `main` before the next crates.io/GitHub release).

## Release note checklist (copy for ship)

Use when cutting a release that touches trust artifacts or packs:

- [ ] **Trust Card** `schema_version` stated (expect **2** for current line).
- [ ] **Claim count** (**7**) and **stable claim `id` values** listed or linked to this doc; remind consumers: **key by `id`**, not index.
- [ ] **Pack engine** version (**1.2**) and mention of `g3_authorization_context_present` if relevant to users.
- [ ] **First tag / version** that includes built-in `mcp-signal-followup` (if this release is the first).
- [ ] **First tag / version** that includes built-in `a2a-signal-followup` (P2b; if this release is the first).
- [ ] **`assay_min_version`** on `mcp-signal-followup`: prerequisite substrate vs first binary-with-pack — one sentence, consistent with [PLAN-P2a](PLAN-P2a-MCP-SIGNAL-FOLLOWUP-CLAIM-PACK.md).
- [ ] **`assay_min_version`** on `a2a-signal-followup`: same as above for P2b — consistent with [PLAN-P2b](PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md).
- [ ] **First tag / version** that includes built-in `a2a-discovery-card-followup` (P2c; if this release is the first).
- [ ] **`assay_min_version`** on `a2a-discovery-card-followup`: **G4-A-capable `>=3.3.0`** vs first binary-with-pack — one sentence, consistent with [PLAN-P2c](PLAN-P2c-A2A-DISCOVERY-CARD-FOLLOWUP-PACK.md).

## Regenerating demo bundles (canonical demo path)

For G3 / P2a CLI demos, the repo uses an **ignored** test that writes `.tar.gz` files under `target/mcp-lint-demo/`:

```bash
cargo test -p assay-evidence --test mcp_signal_followup_pack write_mcp_lint_demo_bundles -- --ignored --nocapture
./target/debug/assay evidence lint target/mcp-lint-demo/g3_full_pass.tar.gz --pack mcp-signal-followup
```

Committed byte fixtures are reserved for **small, low-churn** vectors already covered by integration tests; avoid duplicating large demo archives.
