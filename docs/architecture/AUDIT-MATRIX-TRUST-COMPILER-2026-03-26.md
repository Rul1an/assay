# Trust Compiler Audit Matrix (2026-03-26)

This matrix records the trust-compiler line that moved from `T1b` through `K2-A` Phase 1, plus the
discovery-only ranking that led first to `K1` and then to `K2`. It is meant as an audit aid: each row captures the wave, the PRs
that carried it, the kernel files, the contract/result that matters, and whether that row is
shipped on `main` and/or part of the public `v3.3.0` / `v3.4.0` / `v3.5.0` release truth.

| Wave | PR(s) | Kernbestanden | Belangrijk contract / resultaat | `main` shipped? | Release-truth? |
|------|-------|---------------|----------------------------------|-----------------|----------------|
| `T1b` | `#923` | `crates/assay-evidence/src/trust_card.rs`, `crates/assay-cli/src/cli/commands/trustcard.rs` | Deterministic `trustcard.json` / `trustcard.md` derived from `trust-basis.json`; geen tweede semantische classificatielaag | Yes | Yes — in `v3.3.0` |
| `G3` | `#929` | `crates/assay-evidence/src/g3_authorization_context.rs`, `crates/assay-evidence/src/trust_basis.rs`, `crates/assay-evidence/src/trust_card.rs` | `authorization_context_visible`; Trust Basis met **7 claims**; Trust Card `schema_version: 2`; bounded `auth_scheme` / `auth_issuer` / `principal` op ondersteunde MCP decision evidence | Yes | Yes — in `v3.3.0` |
| Outward positioning | `#930` | `README.md`, trust-compiler docs slices | README / outward positioning verschuift naar claims-as-code / trust compiler taal | Yes | Yes — public docs line through `v3.4.0` |
| `P2a` + engine `1.2` | `#935` | `crates/assay-evidence/src/lint/packs/checks.rs`, `crates/assay-evidence/src/lint/packs/mod.rs`, `packs/open/mcp-signal-followup/pack.yaml` | Built-in `mcp-signal-followup`; `g3_authorization_context_present`; pack engine `1.2` | Yes | Yes — in `v3.3.0` |
| `H1` | `#937` | `docs/architecture/MIGRATION-TRUST-COMPILER-3.2.md`, `crates/assay-evidence/tests/h1_trust_kernel_alignment.rs` | Migration SSOT; kernel / Trust Basis / Trust Card alignment hardening | Yes | Yes — in `v3.3.0` |
| `H1a` | `#938` | shared trust-kernel test vectors and companion tests | Shared vectors and alignment support without new product semantics | Yes | Yes — in `v3.3.0` |
| Roadmap sync | `#939` | `docs/ROADMAP.md` | Roadmap sync after `P2a` / `H1` / `P2b` ordering hardens | Yes | Yes — public docs line through `v3.4.0` |
| `P2b` | `#940` | `packs/open/a2a-signal-followup/pack.yaml`, built-in pack wiring | Built-in `a2a-signal-followup` (`A2A-001..003`); presence-only on canonical A2A adapter evidence | Yes | Yes — in `v3.3.0` |
| `v3.3.0` line | release cut + `#941` hardening | `CHANGELOG.md`, `docs/architecture/RELEASE-PLAN-TRUST-COMPILER-3.3.md`, crates.io line | First coherent public trust-compiler line: Trust Basis, Trust Card v2 / 7 claims, `G3`, engine `1.2`, built-in `P2a` + `P2b`, migration SSOT | Yes | Yes — tag / GitHub release / crates.io |
| `G4` planning | `#942`, `#943` | `docs/architecture/PLAN-G4-A2A-DISCOVERY-CARD-EVIDENCE-2026q2.md`, `docs/architecture/G4-A-PHASE1-FREEZE.md` | Discovery/card evidence wave framed as adapter-first evidence seam, not pack-first expansion | Yes | Yes — public docs line through `v3.4.0` |
| `G4-A` Phase 1 | `#944`, `#945` | `crates/assay-adapter-a2a/src/adapter_impl/discovery.rs`, A2A payload wiring | New typed `payload.discovery` seam for A2A discovery/card visibility | Yes | Yes — in `v3.4.0` |
| `P2c` planning + freeze | `#946`, `#947`, `#948` | `docs/architecture/PLAN-P2c-A2A-DISCOVERY-CARD-FOLLOWUP-PACK.md` and related status docs | `P2c` frozen as a follow-on pack to `G4-A`, not part of `G4` itself | Yes | Yes — public docs line through `v3.4.0` |
| `P2c` | `#949` | built-in `a2a-discovery-card-followup`, pack checks, open mirror | Built-in `A2A-DC-001` / `A2A-DC-002`; `json_path_exists` + `value_equals`; `requires.assay_min_version: ">=3.3.0"` | Yes | Yes — in `v3.4.0` |
| `K1` formalization | `#989`, `#990`, `#991` | `docs/architecture/PLAN-K1-A2A-HANDOFF-DELEGATION-ROUTE-EVIDENCE-2026q2.md`, `docs/ROADMAP.md`, `docs/architecture/RFC-005-trust-compiler-mvp-2026q2.md` | Next formal wave chosen as bounded A2A handoff / delegation-route visibility evidence; explicitly adapter-first, evidence-first, no pack in the same wave | Yes | Yes — public docs line through `v3.4.0` |
| `K1-A` Phase 1 freeze + first adapter slice + closure | `#992`, `#994`, `#995` | `docs/architecture/K1-A-PHASE1-FREEZE.md`, `crates/assay-adapter-a2a/src/adapter_impl/handoff.rs`, `crates/assay-adapter-a2a/src/adapter_impl/payload.rs`, `crates/assay-adapter-a2a/src/adapter_impl/tests.rs`, trust-sync docs | Top-level `payload.handoff` seam always present; `typed_payload` / `unknown` only; positive only for `task.requested` + `task.kind == "delegation"`; explicit non-promotion for `task.updated`, `artifact.shared`, generic-message fallback, synthetic `unknown-task`, and non-`delegation` `task.kind` | Yes | Yes — in `v3.4.0` |
| `K2` planning + `K2-A` freeze | docs-only follow-up on `main` | `docs/architecture/PLAN-K2-MCP-AUTHORIZATION-DISCOVERY-EVIDENCE-2026q2.md`, `docs/architecture/K2-A-PHASE1-FREEZE.md`, `docs/architecture/K2-A-PHASE1-FREEZE-PREP.md`, `docs/ROADMAP.md`, `docs/architecture/RFC-005-trust-compiler-mvp-2026q2.md` | `K2` is formalized as the next bounded MCP authorization-discovery wave; `K2-A` freezes one seam, repo-reality-first source classes, and hard non-promotion rules before implementation | Yes | Planned docs line on `main` |
| `K2-A` Phase 1 implementation | `#1004` | `crates/assay-core/src/mcp/types.rs`, `crates/assay-core/src/mcp/parser.rs`, `crates/assay-core/src/mcp/mapper_v2.rs`, `crates/assay-core/tests/mcp_transport_compat.rs`, `crates/assay-cli/tests/mcp_transport_import.rs` | Bounded MCP authorization-discovery seam on imported MCP traces: top-level `episode_start.meta.mcp.authorization_discovery`, visibility-only, positive only for typed `WWW-Authenticate` discovery on supported `401` transport paths; explicit non-promotion without typed runtime-observed provenance | Yes | Yes — in `v3.5.0` |
| Discovery-only next-wave note | discovery note on `main` | `docs/architecture/DISCOVERY-NEXT-EVIDENCE-WAVE-2026Q2.md` | Preferred next evidence-wave candidate: **A2A handoff / delegation-route visibility**; second candidate: **MCP authorization-discovery**; explicitly **not** automatically another pack | Yes | Yes — public docs line / discovery-only context through `v3.4.0` |

## Current call

The trust-compiler line is now public through `K2-A` Phase 1 in **`v3.5.0`**. The current
documentation converges on this call:

- keep `G4-A` limited to post-merge verification / release-truth hygiene
- treat Assay as a **CI-native protocol-governance** and evidence-first product line, not a trust-score-first engine
- current public slice now includes **bounded MCP authorization-discovery visibility evidence** via `K2-A`
- active bounded evidence wave: **`K2` — MCP authorization-discovery evidence**
- `K2-A` Phase 1 is now public in **`v3.5.0`**
- keep protocol packs downstream of first-class evidence seams, not as the default next move

See also:

- [ROADMAP](./../ROADMAP.md)
- [RFC-005](./RFC-005-trust-compiler-mvp-2026q2.md)
- [DISCOVERY — Next Evidence Wave](./DISCOVERY-NEXT-EVIDENCE-WAVE-2026Q2.md)
- [PLAN — K1](./PLAN-K1-A2A-HANDOFF-DELEGATION-ROUTE-EVIDENCE-2026q2.md)
- [PLAN — K2](./PLAN-K2-MCP-AUTHORIZATION-DISCOVERY-EVIDENCE-2026q2.md)
- [K2-A — Phase 1 formal freeze](./K2-A-PHASE1-FREEZE.md)
- [K2-A — Phase 1 freeze prep](./K2-A-PHASE1-FREEZE-PREP.md)
