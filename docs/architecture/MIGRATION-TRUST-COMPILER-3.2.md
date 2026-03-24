# Migration & release truth — Trust Compiler 3.2 line

**Single source of truth (SSOT)** for Trust Basis, Trust Card, pack engine, and `mcp-signal-followup` contract floors. Other docs (CHANGELOG, README, [PLAN-P2a](PLAN-P2a-MCP-SIGNAL-FOLLOWUP-CLAIM-PACK.md)) point here instead of duplicating version semantics.

For the hardening wave that introduced this document, see [PLAN-H1 — Trust Kernel Alignment & Release Hardening](PLAN-H1-TRUST-KERNEL-ALIGNMENT-RELEASE-HARDENING.md).

## Consumer rule: key by `claim.id`, not position

**Downstream consumers must select trust claims by stable `id` (e.g. `authorization_context_visible`), not by row index or implicit “seventh row”.** Claim order and count may evolve with schema bumps; `id` is the stable contract.

## Trust Card

| Field | Value |
|-------|--------|
| `schema_version` | **2** (adds G3 `authorization_context_visible` in the same row model as v1) |
| Claim rows | **7** fixed `TrustBasisClaim` entries from `generate_trust_basis` |
| Semantics | Trust Card adds **no** claim classification beyond Trust Basis: `trust_basis_to_trust_card` copies `claims` and adds frozen `non_goals` only. |

## Pack engine (evidence lint)

| Item | Value |
|------|--------|
| `ENGINE_VERSION` | **1.2** (`crates/assay-evidence/src/lint/packs/checks.rs`) |
| New check type | `g3_authorization_context_present` (same G3 v1 predicate as Trust Basis `authorization_context_visible` when verified) |

## `mcp-signal-followup` built-in pack

- **Name / version:** `mcp-signal-followup` @ `1.0.0`
- **`requires.assay_min_version`:** `>=3.2.3` tracks the **first released Assay line** with G3 + Trust Card schema prerequisites on the evidence substrate (**v3.2.3** is the reference tag for that prerequisite availability, not necessarily the first binary that embeds the built-in pack).
- **Built-in pack + engine 1.2** ship with the Assay **release that contains P2a**; confirm the **first published version/tag** that embeds `mcp-signal-followup` in release notes.

Details and options (bump floor vs document-only): [PLAN-P2a](PLAN-P2a-MCP-SIGNAL-FOLLOWUP-CLAIM-PACK.md) § `assay_min_version`.

## Release note checklist (copy for ship)

Use when cutting a release that touches trust artifacts or packs:

- [ ] **Trust Card** `schema_version` stated (expect **2** for current line).
- [ ] **Claim count** (**7**) and **stable claim `id` values** listed or linked to this doc; remind consumers: **key by `id`**, not index.
- [ ] **Pack engine** version (**1.2**) and mention of `g3_authorization_context_present` if relevant to users.
- [ ] **First tag / version** that includes built-in `mcp-signal-followup` (if this release is the first).
- [ ] **`assay_min_version`** on `mcp-signal-followup`: prerequisite substrate vs first binary-with-pack — one sentence, consistent with [PLAN-P2a](PLAN-P2a-MCP-SIGNAL-FOLLOWUP-CLAIM-PACK.md).

## Regenerating demo bundles (canonical demo path)

For G3 / P2a CLI demos, the repo uses an **ignored** test that writes `.tar.gz` files under `target/mcp-lint-demo/`:

```bash
cargo test -p assay-evidence --test mcp_signal_followup_pack write_mcp_lint_demo_bundles -- --ignored --nocapture
./target/debug/assay evidence lint target/mcp-lint-demo/g3_full_pass.tar.gz --pack mcp-signal-followup
```

Committed byte fixtures are reserved for **small, low-churn** vectors already covered by integration tests; avoid duplicating large demo archives.
