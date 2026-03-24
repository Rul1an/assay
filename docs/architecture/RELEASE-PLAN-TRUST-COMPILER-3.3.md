# Release plan — Trust Compiler product line (post–P2b merge)

**Purpose:** Cut **one deliberate minor release** after **`P2b`** merges — not because P2b is large in isolation, but because the **trust-compiler story is now coherent in one public line**: **P2a + H1 (+ H1a) + P2b**, with a single migration and consumption story.

**Status:** **v3.3.0** workspace bump and changelog land with this line; tag **`v3.3.0`** to run [`.github/workflows/release.yml`](../../.github/workflows/release.yml) (or `workflow_dispatch` with `version: v3.3.0`).

**SSOT (do not re-invent semantics here):**

- [MIGRATION-TRUST-COMPILER-3.2.md](MIGRATION-TRUST-COMPILER-3.2.md) — contract floors, claim selection rule, pack engine, release checklist copy.
- [CHANGELOG.md](../../CHANGELOG.md) — factual shipped items (pull wording from Unreleased into the version section when tagging).
- [PLAN-P2a](PLAN-P2a-MCP-SIGNAL-FOLLOWUP-CLAIM-PACK.md), [PLAN-P2b](PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md), [PLAN-H1](PLAN-H1-TRUST-KERNEL-ALIGNMENT-RELEASE-HARDENING.md) — pack identity, `assay_min_version` discipline, alignment scope.

The migration filename says **“3.2 line”** (historical); the **Assay workspace semver** for this cut is advised as **3.3.0** below — the SSOT file remains the same document until a separate rename decision.

---

## Framing (one paragraph for release notes)

**Suggested lead:**

> This release completes the **first trust-compiler product line**: canonical **Trust Basis**, **Trust Card** with explicit evidence-level claims, **G3** authorization-context evidence on supported MCP paths, **pack engine 1.2**, built-in **MCP** and **A2A** signal-followup packs, and the **migration / release truth** needed to consume them safely. Later waves (e.g. P2c) can branch from a clear public baseline.

Avoid “big bang because P2b”; use “coherent line is complete.”

---

## Version advice: **3.3.0** (minor), not another 3.2.x patch

**Rationale:** Outward-facing contract surface is larger than a patch-only release:

| Area | Why it fits a **minor** bump |
|------|------------------------------|
| Trust Card | `schema_version` **2**, **seven** claims (G3 row) |
| Evidence / lint | Pack engine **1.2**, `g3_authorization_context_present` |
| Built-in packs | First release line that **ships both** `mcp-signal-followup` and `a2a-signal-followup` in binaries (confirm at tag time) |
| Consumers | **Key claims by `claim.id`**, not row position — document explicitly |

Patch releases remain for fixes-only; **3.3.0** signals “trust compiler line is packaged for downstream.”

**Concrete tagging:** workspace `[workspace.package] version` and `[workspace.dependencies]` internal crate pins → **3.3.0** (root `Cargo.toml`), then tag **`v3.3.0`** per [`.github/workflows/release.yml`](../../.github/workflows/release.yml).

---

## Pre-release verification (must pass)

### 1. Built-ins really ship in the release binary line

- [ ] `BUILTIN_PACKS` in `assay-evidence` includes `mcp-signal-followup` and `a2a-signal-followup` (see `crates/assay-evidence/src/lint/packs/mod.rs`).
- [ ] Open mirrors under `packs/open/*/pack.yaml` are **byte-identical** to built-in YAML where applicable (parity tests).
- [ ] `cargo test -p assay-evidence --test mcp_signal_followup_pack` and `--test a2a_signal_followup_pack` pass on the release commit.

### 2. Docs / changelog / SSOT aligned

- [ ] [CHANGELOG.md](../../CHANGELOG.md): move or summarize trust-compiler items from **Unreleased** into **`## [3.3.0] — YYYY-MM-DD`** (date at tag time).
- [ ] [MIGRATION-TRUST-COMPILER-3.2.md](MIGRATION-TRUST-COMPILER-3.2.md): release note checklist section filled for **this** tag (first binary with `a2a-signal-followup` if applicable; same substrate-floor wording as PLAN-P2a/P2b).
- [ ] No contradiction between ROADMAP “shipped” lines and what the tag actually contains.

### 3. Release notes content (from SSOT, not from memory)

Copy structure from:

- **MIGRATION** — Trust Card table (`schema_version` **2**, **7** claims, `claim.id` rule), pack engine **1.2**, `g3_authorization_context_present`.
- **PLAN-P2a / PLAN-P2b** — pack names, rule IDs, `assay_min_version` meaning (**substrate floor** vs **first binary with built-in pack**).
- **CHANGELOG** — user-visible bullets.

---

## Release notes outline (bullets for users & integrators)

### Users (CLI / bundles)

- Trust Basis compiler output remains the canonical input to Trust Card generation; see `assay trust-basis generate` and related CLI.
- Trust Card **`schema_version: 2`** with **seven** trust-basis claims; see SSOT for stable **`claim.id`** values — **do not** rely on row order or “row count = 7” as a permanent API.
- Evidence lint: pack engine **1.2**; MCP companion pack **`mcp-signal-followup`** (MCP-001..003); A2A companion pack **`a2a-signal-followup`** (A2A-001..003) — bounded presence on adapter-emitted canonical types for P2b.

### Integrators (policy / CI / downstream)

- **Claim selection:** key by **`claim.id`**, not table index — [MIGRATION](MIGRATION-TRUST-COMPILER-3.2.md) consumer contract.
- **`assay_min_version` on packs:** `>=3.2.3` expresses **evidence-substrate** prerequisites; the **first GitHub / crates.io / binary release** that embeds a given built-in pack must be stated in release notes (see PLAN-P2a § `assay_min_version`, PLAN-P2b § `assay_min_version`).
- **G3 / MCP-001:** `g3_authorization_context_present` aligns with Trust Basis `authorization_context_visible` (**verified**) — not authorization *validity*; see PLAN-P2a.

### What this release is not

- Not a “marketing mega-release” — a **substantive line release** with clear contracts.
- P2b does not add engine version bumps; A2A rules are presence-only — see PLAN-P2b.

---

## Suggested CHANGELOG structure for `3.3.0`

```markdown
## [3.3.0] — YYYY-MM-DD

### Trust Compiler (product line)

- Summarize: Trust Basis + Trust Card v2 + seven claims + G3 + pack engine 1.2 + built-in MCP and A2A packs + migration SSOT — with links to MIGRATION and PLAN-P2a / PLAN-P2b.
- Short pointer: H1 / H1a alignment and shared test vectors (if shipped in this line).

### Notes for upgraders

- Bullet: key claims by `claim.id` (link MIGRATION).
- Bullet: `assay_min_version` on packs vs first binary with pack (link PLAN-P2a / PLAN-P2b).

```

Pull detailed bullets from **CHANGELOG Unreleased** and dedupe against this outline.

---

## After this release

- **P2c** (or next protocol slice) can assume **3.3.0+** as the public baseline for trust-compiler artifacts.
- Revisit pack `assay_min_version` floors only when intentionally changing evidence substrate — keep one sentence in release notes per MIGRATION checklist.

---

## References

- [RFC-005 §6](RFC-005-trust-compiler-mvp-2026q2.md) — sequencing context.
- [`.github/workflows/release.yml`](../../.github/workflows/release.yml) — tag and `workflow_dispatch` inputs.
