# Wave8A Step2 Review Pack - A2A Mechanical Split

## Intent

Mechanically split `assay-adapter-a2a` hotspot into bounded internal modules while keeping facade/API behavior frozen.

## Scope

- `crates/assay-adapter-a2a/src/lib.rs`
- `crates/assay-adapter-a2a/src/adapter_impl/*`
- `docs/contributing/SPLIT-MOVE-MAP-wave8a-step2-a2a.md`
- `docs/contributing/SPLIT-CHECKLIST-wave8a-step2-a2a.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave8a-step2-a2a.md`
- `scripts/ci/review-wave8a-step2.sh`

## Behavior freeze proof points

- Same adapter metadata and capabilities surface.
- Same strict/lenient error contracts.
- Same event mapping and payload shaping semantics.
- Same fixture/property test coverage (moved to `adapter_impl/tests.rs`).

## Validation command

```bash
BASE_REF=origin/main bash scripts/ci/review-wave8a-step2.sh
```

## Reviewer 60s scan

1. `lib.rs` only delegates to `adapter_impl`.
2. Move-map matches actual module ownership.
3. reviewer script enforces allowlist/workflow-ban/single-source boundaries.
4. contract anchors pass.
