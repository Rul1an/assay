# Wave55 Mastra ScoreEvent Split Checklist

Scope lock:
- Mechanical split only for `crates/assay-cli/src/cli/commands/evidence/mastra_score_event.rs`.
- Keep the public clap args, command entrypoint, stderr message, receipt schema, JSON payload shape, validation errors, and tests stable.
- No Cargo, workflow, release, schema, Trust Basis, or runtime behavior changes.

Artifacts:
- `docs/contributing/SPLIT-MOVE-MAP-wave55-mastra-score-event.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave55-mastra-score-event.md`
- `scripts/ci/review-wave55-mastra-score-event.sh`

Contract anchors:
- `import_writes_verifiable_score_event_bundle`
- `import_rejects_raw_metadata_and_correlation_context`
- `import_rejects_raw_callback_score_object`
- `import_rejects_missing_scorer_identity`
- `import_rejects_legacy_underscore_surface`

Reviewer gates:
- Facade stays thin and keeps `MastraScoreEventArgs` + `cmd_mastra_score_event`.
- Moved modules own constants, JSONL event reading, reduction, source/provenance helpers, validation, and tests.
- No workflows, Cargo files, schema files, runner/eBPF files, or unrelated evidence importers are touched.

Runbook:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave55-mastra-score-event.sh
```

Definition of done:
- Review script passes.
- `cargo fmt --check`, `cargo check -p assay-cli`, targeted Mastra importer tests, and `cargo clippy -p assay-cli --all-targets -- -D warnings` pass.
- LOC delta is reported in the PR.
