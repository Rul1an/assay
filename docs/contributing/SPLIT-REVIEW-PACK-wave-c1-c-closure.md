# Wave C1 Closure Review Pack

## Intent

Close Wave C1 with a final module map and closure gate that verifies the split landed cleanly and reviewably.

## Scope

- `docs/contributing/SPLIT-CHECKLIST-wave-c1-c-closure.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave-c1-final.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave-c1-c-closure.md`
- `scripts/ci/review-wave-c1-c-closure.sh`

## Non-goals

- No production code changes.
- No workflow changes.

## Validation Command

```bash
BASE_REF=<c1-b3-commit> bash scripts/ci/review-wave-c1-c-closure.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-cli --all-targets -- -D warnings
cargo test -p assay-cli
```

## Reviewer 60s Scan

1. Confirm closure diff is docs/scripts only.
2. Confirm final layout map is concrete and actionable.
3. Run closure script and confirm PASS.
