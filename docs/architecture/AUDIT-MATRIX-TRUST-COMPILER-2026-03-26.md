# Trust Compiler Audit Matrix (2026-03-26)

This matrix records the trust-compiler line that moved from `T1b` through `P2c`, plus the current
post-`P2c` discovery-only note. It is meant as an audit aid: each row captures the wave, the PRs
that carried it, the kernel files, the contract/result that matters, and whether that row is
shipped on `main` and/or part of the public `v3.3.0` release truth.

| Wave | PR(s) | Kernbestanden | Belangrijk contract / resultaat | `main` shipped? | Release-truth? |
|------|-------|---------------|----------------------------------|-----------------|----------------|
| `T1b` | `#923` | `crates/assay-evidence/src/trust_card.rs`, `crates/assay-cli/src/cli/commands/trustcard.rs` | Deterministic `trustcard.json` / `trustcard.md` derived from `trust-basis.json`; geen tweede semantische classificatielaag | Yes | Yes — in `v3.3.0` |
| `G3` | `#929` | `crates/assay-evidence/src/g3_authorization_context.rs`, `crates/assay-evidence/src/trust_basis.rs`, `crates/assay-evidence/src/trust_card.rs` | `authorization_context_visible`; Trust Basis met **7 claims**; Trust Card `schema_version: 2`; bounded `auth_scheme` / `auth_issuer` / `principal` op ondersteunde MCP decision evidence | Yes | Yes — in `v3.3.0` |
| Outward positioning | `#930` | `README.md`, trust-compiler docs slices | README / outward positioning verschuift naar claims-as-code / trust compiler taal | Yes | Main only |
| `P2a` + engine `1.2` | `#935` | `crates/assay-evidence/src/lint/packs/checks.rs`, `crates/assay-evidence/src/lint/packs/mod.rs`, `packs/open/mcp-signal-followup/pack.yaml` | Built-in `mcp-signal-followup`; `g3_authorization_context_present`; pack engine `1.2` | Yes | Yes — in `v3.3.0` |
| `H1` | `#937` | `docs/architecture/MIGRATION-TRUST-COMPILER-3.2.md`, `crates/assay-evidence/tests/h1_trust_kernel_alignment.rs` | Migration SSOT; kernel / Trust Basis / Trust Card alignment hardening | Yes | Yes — in `v3.3.0` |
| `H1a` | `#938` | shared trust-kernel test vectors and companion tests | Shared vectors and alignment support without new product semantics | Yes | Yes — in `v3.3.0` |
| Roadmap sync | `#939` | `docs/ROADMAP.md` | Roadmap sync after `P2a` / `H1` / `P2b` ordering hardens | Yes | Main only |
| `P2b` | `#940` | `packs/open/a2a-signal-followup/pack.yaml`, built-in pack wiring | Built-in `a2a-signal-followup` (`A2A-001..003`); presence-only on canonical A2A adapter evidence | Yes | Yes — in `v3.3.0` |
| `v3.3.0` line | release cut + `#941` hardening | `CHANGELOG.md`, `docs/architecture/RELEASE-PLAN-TRUST-COMPILER-3.3.md`, crates.io line | First coherent public trust-compiler line: Trust Basis, Trust Card v2 / 7 claims, `G3`, engine `1.2`, built-in `P2a` + `P2b`, migration SSOT | Yes | Yes — tag / GitHub release / crates.io |
| `G4` planning | `#942`, `#943` | `docs/architecture/PLAN-G4-A2A-DISCOVERY-CARD-EVIDENCE-2026q2.md`, `docs/architecture/G4-A-PHASE1-FREEZE.md` | Discovery/card evidence wave framed as adapter-first evidence seam, not pack-first expansion | Yes | Main only |
| `G4-A` Phase 1 | `#944`, `#945` | `crates/assay-adapter-a2a/src/adapter_impl/discovery.rs`, A2A payload wiring | New typed `payload.discovery` seam for A2A discovery/card visibility | Yes | Main only (post-`v3.3.0`) |
| `P2c` planning + freeze | `#946`, `#947`, `#948` | `docs/architecture/PLAN-P2c-A2A-DISCOVERY-CARD-FOLLOWUP-PACK.md` and related status docs | `P2c` frozen as a follow-on pack to `G4-A`, not part of `G4` itself | Yes | Main only |
| `P2c` | `#949` | built-in `a2a-discovery-card-followup`, pack checks, open mirror | Built-in `A2A-DC-001` / `A2A-DC-002`; `json_path_exists` + `value_equals`; `requires.assay_min_version: ">=3.3.0"` | Yes | Main only (after `v3.3.0`) |
| Discovery-only next-wave note | discovery note on `main` | `docs/architecture/DISCOVERY-NEXT-EVIDENCE-WAVE-2026Q2.md` | Preferred next evidence-wave candidate: **A2A handoff / delegation-route visibility**; second candidate: **MCP authorization-discovery**; explicitly **not** automatically another pack | Yes | Main only / discovery-only |

## Current call

The trust-compiler line on `main` is shipped through `P2c`. The next formal wave is **not** yet a
new `P` slice by default. The current documentation now converges on this call:

- keep `G4-A` limited to post-merge verification / release-truth hygiene
- treat the next step as a **decision point**, not as “automatic `P2d`”
- current best candidate: **bounded A2A handoff / delegation-route visibility evidence**
- current second candidate: **MCP authorization-discovery evidence**

See also:

- [ROADMAP](./../ROADMAP.md)
- [RFC-005](./RFC-005-trust-compiler-mvp-2026q2.md)
- [DISCOVERY — Next Evidence Wave](./DISCOVERY-NEXT-EVIDENCE-WAVE-2026Q2.md)
