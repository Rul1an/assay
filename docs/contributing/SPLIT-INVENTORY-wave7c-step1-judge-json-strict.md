# Wave7C Step1 Inventory: judge + json_strict freeze

Intent:
- Behavior freeze and reviewer-gate baseline before any mechanical split for:
  - `crates/assay-core/src/judge/mod.rs`
  - `crates/assay-evidence/src/json_strict/mod.rs`

Snapshot:
- Baseline commit (origin/main at authoring): `f42c20db`

LOC snapshot:
- `crates/assay-core/src/judge/mod.rs`: `712`
- `crates/assay-evidence/src/json_strict/mod.rs`: `759`

Hotspot rationale:
- `judge/mod.rs`: mixed orchestration + prompt/build + reliability loop + cache/meta wiring in one file.
- `json_strict/mod.rs`: strict parser state-machine + tests in same module; strong security boundary with duplicate-key rejection.

Wave7C Step1 scope lock:
- tests + docs + reviewer gates only.
- no mechanical move.
- no behavior/perf/API changes.
