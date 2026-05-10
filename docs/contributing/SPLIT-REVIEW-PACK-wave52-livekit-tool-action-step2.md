# SPLIT REVIEW PACK - Wave 52 LiveKit Tool Action Step2

## Summary

Step2 mechanically splits the LiveKit tool-action importer after the Step1
alignment/freeze landed. The public CLI facade stays stable while parsing,
reduction, validation, canonical hashing, bundling, constants, and tests move
into dedicated modules.

## Included

- Facade reduction from `1095` LOC to `81` LOC.
- New `livekit_tool_action/` helper module directory.
- Step2 checklist, move map, review pack, and reviewer gate.

## Excluded

- CLI argument changes.
- Receipt schema or input schema changes.
- Receipt payload shape changes.
- Pairing/null-output behavior changes.
- Trust Basis behavior changes.
- Workflow changes.
- Public family-matrix or Trust Basis claim additions.

## Validation

Run:

```bash
bash scripts/ci/review-wave52-livekit-tool-action-step2.sh
```

The script checks:

- allowlist-only diff
- no workflow/generated/schema edits
- facade LOC below 150
- required module files exist
- frozen LiveKit surface markers remain present
- `cargo fmt --check`
- `cargo check -p assay-cli`
- `cargo clippy -p assay-cli --all-targets -- -D warnings`
- `cargo test -q -p assay-cli livekit_tool_action`
- Trust Basis non-mutation integration test
- `git diff --check`

## Next Step

Stop after this Step2 split unless concrete review pain appears. The facade is
below the Wave52 stop-rule threshold, and the fresh importer now has bounded
module seams.
