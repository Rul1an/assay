# Writer split Step 3 checklist and gates

Scope lock:
- Scope: refactor + docs + gates only.
- No semantic changes, no perf changes in Step 3 Commit A/B/C.
- `demo/` untouched.

## Commit slicing

- Commit A: `writer_next/*` scaffold only (not wired), explicit boundaries.
- Commit B: mechanical 1:1 moves behind existing facade in `writer.rs`.
- Commit C: review artifacts + hard-fail grep gates + reviewer script.

## Target layout

```text
crates/assay-evidence/src/bundle/writer_next/
  mod.rs
  write.rs
  verify.rs
  manifest.rs
  events.rs
  tar_write.rs
  tar_read.rs
  limits.rs
  errors.rs
  tests.rs
```

## Public surface freeze

Source of truth: `docs/contributing/SPLIT-MOVE-MAP-writer-step3.md`.

Public symbols/signatures in `writer.rs` must remain stable for Step 3.

## Boundary gates (copy/paste)

Run with `bash` + `set -euo pipefail`. These checks must fail when a forbidden match is present.

```bash
check_no_match() {
  local pattern="$1"
  local path="$2"
  local rg_bin
  rg_bin="$(command -v rg)"
  if "$rg_bin" -n "$pattern" "$path"; then
    echo "Forbidden match in $path (pattern: $pattern)"
    exit 1
  fi
}

# 1) verify orchestration must not pull write-path machinery
check_no_match "BundleWriter|write_entry|tar_write|append\(|GzEncoder|tar::Builder|HeaderMode::Deterministic" \
  crates/assay-evidence/src/bundle/writer_next/verify.rs

# 2) write orchestration must not own verify decision logic
check_no_match "verify_bundle|verify_bundle_with_limits|VerifyLimits|ErrorCode::(Security|Contract|Integrity|Limit)" \
  crates/assay-evidence/src/bundle/writer_next/write.rs

# 3) errors module stays mapping/types-only
check_no_match "tar::|flate2::|std::fs|tokio::fs|PathBuf" \
  crates/assay-evidence/src/bundle/writer_next/errors.rs

# 4) limits are single-source; no MAX_* constants outside limits.rs
check_no_match "const\s+MAX_" \
  crates/assay-evidence/src/bundle/writer_next/{write.rs,verify.rs,manifest.rs,events.rs,tar_write.rs,tar_read.rs,errors.rs,mod.rs}

# 5) tar write/read split discipline
check_no_match "set_mtime|set_uid|set_gid|set_username|set_groupname|HeaderMode::Deterministic|GzBuilder|Compression|tar::Builder" \
  crates/assay-evidence/src/bundle/writer_next/tar_read.rs

check_no_match "tar::Archive|entries\(|components\(|strip_prefix\(|starts_with\(" \
  crates/assay-evidence/src/bundle/writer_next/tar_write.rs
```

## Reviewer script

```bash
set -euo pipefail

check_no_match() {
  local pattern="$1"
  local path="$2"
  local rg_bin
  rg_bin="$(command -v rg)"
  if "$rg_bin" -n "$pattern" "$path"; then
    echo "Forbidden match in $path (pattern: $pattern)"
    exit 1
  fi
}

cargo fmt --check
cargo clippy -p assay-evidence --all-targets -- -D warnings
cargo check -p assay-evidence

# Existing high-risk writer contract tests
cargo test -p assay-evidence test_manifest_first -- --nocapture
cargo test -p assay-evidence test_verify_limits_overrides_drift_guard -- --nocapture
cargo test -p assay-evidence test_size_integrity_mismatch -- --nocapture

# Boundary gates
check_no_match "BundleWriter|write_entry|tar_write|append\(|GzEncoder|tar::Builder|HeaderMode::Deterministic" crates/assay-evidence/src/bundle/writer_next/verify.rs
check_no_match "verify_bundle|verify_bundle_with_limits|VerifyLimits|ErrorCode::(Security|Contract|Integrity|Limit)" crates/assay-evidence/src/bundle/writer_next/write.rs
check_no_match "tar::|flate2::|std::fs|tokio::fs|PathBuf" crates/assay-evidence/src/bundle/writer_next/errors.rs
check_no_match "const\s+MAX_" crates/assay-evidence/src/bundle/writer_next/{write.rs,verify.rs,manifest.rs,events.rs,tar_write.rs,tar_read.rs,errors.rs,mod.rs}
check_no_match "set_mtime|set_uid|set_gid|set_username|set_groupname|HeaderMode::Deterministic|GzBuilder|Compression|tar::Builder" crates/assay-evidence/src/bundle/writer_next/tar_read.rs
check_no_match "tar::Archive|entries\(|components\(|strip_prefix\(|starts_with\(" crates/assay-evidence/src/bundle/writer_next/tar_write.rs
```

## Diff scope check

```bash
# Replace <base> with the base branch ref used for the stacked PR
git diff --stat <base>...HEAD | \
  rg -v "crates/assay-evidence/src/bundle/writer.rs|crates/assay-evidence/src/bundle/writer_next|docs/contributing/SPLIT-CHECKLIST-writer-step3.md|docs/contributing/SPLIT-MOVE-MAP-writer-step3.md|docs/architecture/PLAN-split-refactor-2026q1.md"
# Expect: 0
```

## Definition of done

- Public surface/signatures unchanged.
- Commit A/B/C remain semantic no-op.
- Boundary gates pass.
- Targeted writer contract tests pass.
