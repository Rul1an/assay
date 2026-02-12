# Outstanding TODOs

Tracked work items that are marked in code with `// TODO(tag):` and documented here. When addressing one, remove or update the in-code comment and this row.

## Master list

| Tag | Crate | Location | Summary |
|-----|-------|----------|---------|
| **sandbox-scrub** | assay-cli | `cli/commands/sandbox.rs` | If partial env scrubbing is implemented, set `scrubbed: true` in profiler for keys that were redacted. |
| **sim-verify-limits** | assay-cli | `cli/commands/sim.rs` | Parse `verify_limits` from `args.limits` when present and pass into `SuiteConfig`. |
| **landlock-abi-v5** | assay-cli | `backend.rs` | ABI v5 (IOCTL), v6 (Scoping), v7 (Audit) when landlock crate or raw syscalls support them (SOTA 2026). |
| **landlock-net** | assay-cli | `backend.rs` | Add NET rules (ABI V4) when `abi_level >= 4`; currently FS-only. |
| **validate-v13** | assay-core | `validate/mod.rs` | Full policy-engine context for detailed arg enforcement in trace validation (v1.3). |
| **sequence-v11** | assay-metrics | `sequence_valid.rs` | Implement v1.1 sequence operators (Eventually, MaxCalls, etc.); consider delegating to `assay-core::explain::TraceExplainer` when stable. |

## Placement in roadmap and implementation plan

Where each TODO should be fixed, with priority, value, urgency, and dependencies. Sources: [ROADMAP](../ROADMAP.md), [DX-IMPLEMENTATION-PLAN (archive)](../archive/DX-IMPLEMENTATION-PLAN-legacy.md), [ADR-019 PR Gate 2026 SOTA](../architecture/ADR-019-PR-Gate-2026-SOTA.md).

| Tag | Where to fix | Priority | Value | Urgency | Dependencies |
|-----|--------------|----------|-------|---------|--------------|
| **sandbox-scrub** | **E5 / E9.4** (Privacy, Replay scrubbing). Only when partial scrubbing exists. | P2 | Low until feature exists | Later | Depends on partial env scrubbing implementation. |
| **sim-verify-limits** | **Backlog.** `assay sim` attack simulation; not in DX epics. | Backlog | Low | Later | None. |
| **landlock-abi-v5** | **ROADMAP Backlog:** “Runtime Extensions (Epic G): ABI 6/7”. | Backlog | Medium | Later | Landlock crate or kernel support for ABI v5/v6/v7. |
| **landlock-net** | **ROADMAP Foundation** completion: full ABI V4 (NET). Currently FS-only. | P2 / Backlog | Medium | Later | Landlock crate NET (ABI V4) support. |
| **validate-v13** | **Backlog.** Trace validation v1.3 with full policy context. | Backlog | Medium | Later | Policy engine context available in validate path. |
| **sequence-v11** | **Backlog.** Metrics/sequence DSL v1.1 operators. | Backlog | Medium | Later | Optional: `assay-core::explain::TraceExplainer` stable API. |

### Suggested fix order (by plan phase)

1. **Later / Backlog:** `sandbox-scrub` (after scrubbing), `sim-verify-limits`, `landlock-net`, `landlock-abi-v5`, `validate-v13`, `sequence-v11`.

**Done:** `cli-verify` (P0); `monitor-strict-warn` (P1); `mcp-deny-code` (P1); `mcp-op-class` (P1); `init-provider-template` (P1); `runner-metric-override` (P1 — `Expected::thresholding_for_metric` + per-test max_drop in baseline regression); `writer-split` (E2 — assay-evidence bundle split: manifest, limits, verify, write; façade re-exports XAssayExtension, BundleProvenance; public_api_smoke test; ADR-025-E2 bundle_digest logical digest docs).

## Conventions

- **Tag**: Short identifier used in code as `// TODO(tag):` for grep and cross-reference.
- **Crate**: Workspace crate where the TODO lives.
- **Location**: Path under that crate’s `src/` (or `tests/`).
- **Summary**: One-line description; details are in the code at the given file.

When adding a new TODO: add a row above and use `// TODO(new-tag): summary` in the source file.
