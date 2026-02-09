# RFC-002: Code Health Remediation Plan (Q1 2026)

- Status: Proposed
- Date: 2026-02-09
- Owner: DX/Core
- Scope: `assay-cli`, `assay-core`, `assay-evidence`, `assay-metrics`, `assay-sim`, `assay-registry`
- Inputs:
  - `docs/architecture/CODE-ANALYSIS-REPORT.md`
  - `docs/architecture/PLAN-pipeline-decomposition.md`

## 1. Context

After completing the runner/trace decompositions (D1/D2), the remaining technical debt is concentrated in:

1. `store.rs` (duplicatie + timestamp-inconsistentie + structuur)
2. `monitor.rs` (still monolithic; helper extraction in progress)
3. `generate.rs` (grootte/concentratie)
4. `assay-metrics` extractie-duplicatie voor tool calls
5. small but risky inconsistencies (registry digest helpers, pack name validation, deprecated wrappers)

Doel van deze RFC: de resterende punten afbouwen met minimale regressierisico's en maximale reviewbaarheid.

## 2. Verified Snapshot (2026-02-09)

Based on current code inspection:

- P1 findings opgelost:
  - run/ci error-handler duplicatie (pipeline error API)
  - store result rehydration dedup
  - episode graph loader dedup
  - severity model unification + pointer helper vereenvoudiging
  - sim attack bundle dedup
  - trace `from_path` decompositie
  - runner `run_test_with_policy` decompositie
- P1 finding gedeeltelijk:
  - monitor monoliet: helper-extractie gestart, event loop/Tier1-flow nog groot
- Open kern:
  - `store.rs` timestamp canonicalisatie + structuur
  - metrics tool-call extractie duplicatie
  - registry dead/deprecated digest/signature paden
  - `generate.rs` opsplitsing
  - comment/dead-code cleanup batch

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

## 5. Execution Plan (Next Steps)

## E1 - Store Consistency Slice (P1/P2)

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

## E2 - Metrics Extract Helper Slice (P2)

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

## E3 - Registry Dead/Legacy Cleanup (P2/P3)

Scope:

- `compute_digest_raw` / `verify_dsse_signature` rationaliseren
- dubbele digest helper-logica reduceren

Gate:

- `cargo test -p assay-registry -- --nocapture`
- `cargo check -p assay-registry`

Stop-line:

- DSSE verify semantics identiek

## E4 - Comment/Noise/Low-risk Cleanup (P3)

Scope:

- remove stale TODO/debug/comments met onduidelijke intent
- typo/doc drift cleanup
- trivial pass-through wrappers evalueren (`format_plain`, etc.)

Gate:

- crate-local tests + clippy clean

## E5 - Generate Decomposition RFC (separate)

`generate.rs` wordt als aparte traject-RFC behandeld wegens omvang en cross-concern risico.

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

- Open P1 findings uit CODE-ANALYSIS rapport = 0
- Minimaal 6 P2 findings opgelost zonder contractwijziging
- Geen nieuwe monoliet > bestaande baseline in kritieke commands/core paden

## 9. Out of Scope

- Demo assets/workflows in local `demo/` or `.github/workflows/demo.yml`
- Nieuwe productfeatures
- Spec-version bumps voor outputs
