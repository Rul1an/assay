# Wave8B Step2 Review Pack - UCP Mechanical Split

## Intent

Mechanically split `assay-adapter-ucp` hotspot into bounded internal modules while keeping facade/API behavior frozen.

## Scope

- `crates/assay-adapter-ucp/src/lib.rs`
- `crates/assay-adapter-ucp/src/adapter_impl/*`
- `docs/contributing/SPLIT-MOVE-MAP-wave8b-step2-ucp.md`
- `docs/contributing/SPLIT-CHECKLIST-wave8b-step2-ucp.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave8b-step2-ucp.md`
- `scripts/ci/review-wave8b-step2.sh`

## Behavior freeze proof points

- Same adapter metadata and capabilities surface.
- Same strict/lenient error contracts.
- Same event mapping and payload shaping semantics.
- Same fixture/property test coverage (moved to `adapter_impl/tests.rs`).

## Validation command

```bash
BASE_REF=origin/main bash scripts/ci/review-wave8b-step2.sh
```
