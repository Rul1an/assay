# RFC-002: Code Health Remediation Plan (Q1 2026)

- Status: Active (E1-E4 delivered, E5 tracked in RFC-003)
- Date: 2026-02-09
- Owner: DX/Core
- Scope: `assay-cli`, `assay-core`, `assay-evidence`, `assay-metrics`, `assay-sim`, `assay-registry`
- Inputs:
  - `docs/architecture/CODE-ANALYSIS-REPORT.md`
  - `docs/architecture/PLAN-pipeline-decomposition.md`

## 0. Delivery Evidence (mechanical)

| Item | Status | Reference | Merge SHA | Date |
|------|--------|-----------|-----------|------|
| E1 Store consistency | Merged | PR #242 | `d9afdc70` | 2026-02-09 |
| E2 Metrics extraction/dedup | Merged | PR #245 | `39448078` | 2026-02-09 |
| E3 Registry cleanup A | Merged | PR #247 | `ae6e76c4` | 2026-02-09 |
| E3 Registry cleanup B | Merged | PR #250 | `34a03810` | 2026-02-09 |
| E3 Registry cleanup C | Merged | PR #252 | `e06f2458` | 2026-02-09 |
| E4 Comment cleanup A | Merged | PR #253 | `47c9c6b3` | 2026-02-09 |
| E4 Comment cleanup B | Merged | PR #254 | `574d9316` | 2026-02-09 |
| E4 Comment cleanup C | Merged | PR #255 | `c9f67b19` | 2026-02-09 |
| E4 Comment cleanup D | Merged | PR #256 | `54dff1ee` | 2026-02-09 |
| E5 Generate decomposition | Active | RFC-003 / PR #271 | - | Open |

## 1. Context

After completing the runner/trace decompositions (D1/D2), the code-health backlog was concentrated in:

1. `store.rs` (duplicatie + timestamp-inconsistentie + structuur)
2. `monitor.rs` (still monolithic; helper extraction in progress)
3. `generate.rs` (grootte/concentratie)
4. `assay-metrics` extractie-duplicatie voor tool calls
5. small but risky inconsistencies (registry digest helpers, pack name validation, deprecated wrappers)

Doel van deze RFC: de resterende punten afbouwen met minimale regressierisico's en maximale reviewbaarheid.
Current state: E1-E4 are delivered; E5 (`generate.rs` decomposition) moves to a dedicated follow-up RFC.

## 2. Verified Snapshot (2026-02-09, refreshed)

Based on merged slices on `main`:

- P1 findings opgelost:
  - run/ci error-handler duplicatie (pipeline error API)
  - store result rehydration dedup
  - episode graph loader dedup
  - severity model unification + pointer helper vereenvoudiging
  - sim attack bundle dedup
  - trace `from_path` decompositie
  - runner `run_test_with_policy` decompositie
- Delivered in RFC-002 execution:
  - E1 Store consistency (`#242`)
  - E2 Metrics extraction + warning dedup (`#245`, `#246`)
  - E3 Registry digest/signature cleanup (`#247`, `#250`, `#252`)
  - E4 Comment/noise cleanup batch (`#253`, `#254`, `#255`, `#256`)
- Remaining structural backlog outside delivered E1-E4:
  - `monitor.rs` monolith (helper extraction is partial)
  - `generate.rs` decomposition (moved to RFC-003)

## 3. Research Baseline (Best Practices, SOTA, Feb 2026)

Sources (primary docs):

1. Rust API Guidelines checklist (`C-CONV`, `C-ITER`, `C-COMMON-TRAITS`) adviseert consistente API-vorming en trait-based conversies.
   - https://rust-lang.github.io/api-guidelines/checklist.html
2. Clippy docs: `cognitive_complexity` is restriction-only en geen betrouwbare hoofdmetric; gebruik vooral concrete lints (`too_many_lines`, `excessive_nesting`, etc.) plus characterization tests.
   - https://rust-lang.github.io/rust-clippy/stable/index.html
3. SQLite datetime guidance: SQLite has no native datetime type; choose and enforce ONE canonical representation (TEXT ISO-8601 or INTEGER unixepoch) per column.
   - https://www.sqlite.org/lang_datefunc.html
   - https://sqlite.org/quirks.html
   - https://www.sqlite.org/stricttables.html
4. Rust 2024 migration/safety lints blijven relevant voor correctness-hygiëne tijdens refactors (`unsafe_op_in_unsafe_fn`, unsafe attrs, edition migration discipline).
   - https://doc.rust-lang.org/edition-guide/
5. Test infra state-of-the-art: `cargo-nextest` remains the de-facto CI test runner for faster, deterministic suites; Miri integration is practical for UB checks on selected paths.
   - https://www.nexte.st/
   - https://www.nexte.st/changelog/
   - https://nexte.st/docs/integrations/miri/

## 4. Decisions

### D1. Refactor policy

For all remaining findings:

- Test-first (behavior freeze) voor functioneel gevoelige paden
- Daarna extract-only
- Pas daarna semantische verbeteringen
- Geen output-contract changes (`run.json`, `summary.json`, SARIF, JUnit) tenzij expliciet geversioneerd

### D2. Timestamp canonicalisatie

For `store.rs`, we choose a canonical timestamp representation for `runs.started_at`:

- Canonical write format: RFC3339 UTC (`Z`) with fixed millisecond precision
- Geen mixed writes (`unix:N` vs RFC3339) meer in dezelfde kolom
- Read-compat blijft intact voor legacy waarden
- All new writes route through one helper (`now_rfc3339ish`) for format consistency

Rationale: aligns with SQLite textual datetime practice and avoids downstream parse ambiguity.

### D3. Complexity governance

Geen "single giant PR" voor monolieten. Maximaal 1 concern per PR, met diffstat en contract-gates in de PR-body.

## 5. Execution Plan (RFC-002 Delivery + Next)

## E1 - Store Consistency Slice (P1/P2) - Delivered

Scope:

- Dedup `insert_run`/`create_run` overlap
- Timestamp write canonicalisatie in `store.rs`
- Eventuele `impl Store` structurering (zonder gedrag te wijzigen)
- Characterization tests first, then extract-only/dedup

Gate:

- `cargo test -p assay-core --test store_consistency_e1 -- --nocapture`
- `cargo test -p assay-core storage -- --nocapture`
- `cargo test -p assay-core --lib -- --nocapture`
- `cargo check -p assay-core`
- `cargo clippy -p assay-core -- -D warnings`

Stop-line:

- Geen schemawijziging
- Geen verandering aan query-semantiek
- No ordering/selection drift: latest-run and run-list selection semantics blijven identiek
- No timestamp-format drift: all new writes use canonical helper format only

E1 characterization contract checklist:

- Freeze hoe "latest run" wordt bepaald (ID-based selection blijft leidend)
- Freeze `insert_run` vs `create_run` invariants (status, suite, config_json behavior)
- Freeze legacy read-compat for `runs.started_at` (`unix:*` values remain readable)
- Freeze canonical timestamp contract (UTC + fixed precision)
- Verify no conflict/side-effect drift from dedup (insert behavior + error path unchanged)

## E2 - Metrics Extract Helper Slice (P2) - Delivered

Scope:

- Shared helper voor `tool_calls` extractie in:
  - `args_valid.rs`
  - `sequence_valid.rs`
  - `tool_blocklist.rs`
- Deprecated warning block dedup (waar zinvol)

Gate:

- `cargo test -p assay-metrics -- --nocapture`
- `cargo check -p assay-metrics`

Stop-line:

- Geen metric contract-wijziging
- Geen score/reason drift

## E3 - Registry Dead/Legacy Cleanup (P2/P3) - Delivered

Scope:

- `compute_digest_raw` / `verify_dsse_signature` rationaliseren
- dubbele digest helper-logica reduceren

Gate:

- `cargo test -p assay-registry -- --nocapture`
- `cargo check -p assay-registry`

Stop-line:

- DSSE verify semantics identiek

## E4 - Comment/Noise/Low-risk Cleanup (P3) - Delivered

Scope:

- remove stale TODO/debug/comments met onduidelijke intent
- typo/doc drift cleanup
- trivial pass-through wrappers evalueren (`format_plain`, etc.)

Gate:

- crate-local tests + clippy clean

## E5 - Generate Decomposition RFC (separate, active follow-up)

`generate.rs` wordt als aparte traject-RFC behandeld wegens omvang en cross-concern risico.
Active follow-up document:

- `docs/architecture/RFC-003-generate-decomposition-q1-2026.md`

## 6. Risk Controls

- Characterization tests vóór elke extractie
- Geen cross-cutting refactors in dezelfde PR
- Per PR expliciet:
  - In-scope
  - Non-goals
  - Contract gates
  - Output impact: "none"

## 7. Merge Strategy

1. Merge blocking behavior fixes first (auto-merge allowed after required checks)
2. Merge extract-only PRs small and linear
3. Keep stacked branches shallow (max 1 afhankelijkheid)
4. Rebase op `main` zodra basis-PR merged is

## 8. Definition of Done (RFC-002)

RFC-002 is "Done" wanneer E1-E4 gemerged zijn en:

- Open P1 findings uit de RFC-002 targetset = 0
- Minimaal 6 P2 findings opgelost zonder contractwijziging
- Geen regressie in output-contracten (`run.json`, `summary.json`, SARIF, JUnit)

## 9. Out of Scope

- Demo assets/workflows in local `demo/` or `.github/workflows/demo.yml`
- Nieuwe productfeatures
- Spec-version bumps voor outputs
