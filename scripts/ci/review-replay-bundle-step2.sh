#!/usr/bin/env bash
set -euo pipefail

base_ref="${BASE_REF:-${1:-}}"
if [ -z "${base_ref}" ] && [ -n "${GITHUB_BASE_REF:-}" ]; then
  base_ref="origin/${GITHUB_BASE_REF}"
fi
if [ -z "${base_ref}" ]; then
  base_ref="origin/main"
fi
if ! git rev-parse --verify --quiet "${base_ref}^{commit}" >/dev/null; then
  echo "BASE_REF not found: ${base_ref}"
  exit 1
fi

echo "BASE_REF=${base_ref} sha=$(git rev-parse "${base_ref}")"
echo "HEAD sha=$(git rev-parse HEAD)"

rg_bin="$(command -v rg)"

echo '== Replay bundle Step2 quality checks =='
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -p assay-core --lib replay::bundle::tests::write_bundle_minimal_roundtrip -- --exact
cargo test -p assay-core --lib replay::bundle::tests::bundle_digest_equals_sha256_of_written_bytes -- --exact
cargo test -p assay-core --lib replay::verify::tests::verify_clean_bundle_passes -- --exact

echo '== Replay bundle Step2 scope checks =='
leaks="$({ git diff --name-only "${base_ref}...HEAD" | \
  "${rg_bin}" -v '^crates/assay-core/src/replay/bundle\.rs$|^crates/assay-core/src/replay/bundle/|^docs/contributing/SPLIT-CHECKLIST-replay-bundle-step2\.md$|^docs/contributing/SPLIT-MOVE-MAP-replay-bundle-step2\.md$|^docs/contributing/SPLIT-REVIEW-PACK-replay-bundle-step2\.md$|^scripts/ci/review-replay-bundle-step2\.sh$' || true; })"
if [ -n "${leaks}" ]; then
  echo 'non-allowlisted files detected:'
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^\.github/workflows/'; then
  echo 'workflow changes are forbidden in replay-bundle Step2'
  exit 1
fi

if git status --porcelain -- crates/assay-core/src/replay/bundle | "${rg_bin}" '^\?\?'; then
  echo 'untracked files under crates/assay-core/src/replay/bundle/** are forbidden in Step2'
  exit 1
fi

echo '== Replay bundle Step2 facade invariants =='
facade='crates/assay-core/src/replay/bundle/mod.rs'
if [ ! -f "${facade}" ]; then
  echo "missing facade file: ${facade}"
  exit 1
fi

facade_loc="$(awk 'NF{c++} END{print c+0}' "${facade}")"
if [ "${facade_loc}" -gt 220 ]; then
  echo "facade LOC budget exceeded (${facade_loc} > 220): ${facade}"
  exit 1
fi

"${rg_bin}" -n '^mod io;\s*$' "${facade}" >/dev/null || { echo "missing 'mod io;'"; exit 1; }
"${rg_bin}" -n '^mod manifest;\s*$' "${facade}" >/dev/null || { echo "missing 'mod manifest;'"; exit 1; }
"${rg_bin}" -n '^mod verify;\s*$' "${facade}" >/dev/null || { echo "missing 'mod verify;'"; exit 1; }
"${rg_bin}" -n '^pub mod paths;\s*$' "${facade}" >/dev/null || { echo "missing 'pub mod paths;'"; exit 1; }
"${rg_bin}" -n '^pub use io::\{read_bundle_tar_gz, write_bundle_tar_gz\};\s*$' "${facade}" >/dev/null || {
  echo 'missing io re-export in facade'
  exit 1
}
"${rg_bin}" -n '^pub use manifest::build_file_manifest;\s*$' "${facade}" >/dev/null || {
  echo 'missing manifest re-export in facade'
  exit 1
}
"${rg_bin}" -n '^pub use verify::bundle_digest;\s*$' "${facade}" >/dev/null || {
  echo 'missing verify re-export in facade'
  exit 1
}

if "${rg_bin}" -n '^\s*(pub\s+)?fn\s+' "${facade}" >/dev/null; then
  echo 'facade must not define functions'
  "${rg_bin}" -n '^\s*(pub\s+)?fn\s+' "${facade}"
  exit 1
fi

if "${rg_bin}" -n 'GzBuilder|GzDecoder|Archive|Builder|Header|Sha256' "${facade}" >/dev/null; then
  echo 'tar/gzip/digest logic markers leaked into facade'
  exit 1
fi

if "${rg_bin}" -n '^\s*mod tests\s*\{' "${facade}" >/dev/null; then
  echo 'inline mod tests { ... } is forbidden in facade'
  exit 1
fi

echo '== Replay bundle Step2 boundary invariants =='
"${rg_bin}" -n 'GzBuilder|GzDecoder|Archive|Builder|Header' crates/assay-core/src/replay/bundle/io.rs >/dev/null || {
  echo 'expected tar/gzip IO markers in io.rs'
  exit 1
}
"${rg_bin}" -n 'Sha256::digest' crates/assay-core/src/replay/bundle/verify.rs >/dev/null || {
  echo 'expected digest marker in verify.rs'
  exit 1
}
"${rg_bin}" -n '^\s*pub\s+const\s+MANIFEST' crates/assay-core/src/replay/bundle/paths.rs >/dev/null || {
  echo 'expected path constants in paths.rs'
  exit 1
}
"${rg_bin}" -n '^\s*pub\s+fn\s+build_file_manifest\(' crates/assay-core/src/replay/bundle/manifest.rs >/dev/null || {
  echo 'expected build_file_manifest in manifest.rs'
  exit 1
}

validate_defs="$({ "${rg_bin}" -n '^\s*(pub\(crate\)|pub\(super\))?\s*\s*fn\s+validate_entry_path\(' crates/assay-core/src/replay/bundle/*.rs || true; })"
validate_count="$(echo "${validate_defs}" | "${rg_bin}" -n '.' | wc -l | tr -d ' ')"
if [ "${validate_count}" -ne 1 ]; then
  echo 'validate_entry_path must be defined exactly once'
  echo "${validate_defs}"
  exit 1
fi

echo '== Replay bundle Step2 test relocation invariants =='
"${rg_bin}" -n '^\s*fn write_bundle_minimal_roundtrip\(' crates/assay-core/src/replay/bundle/tests.rs >/dev/null
"${rg_bin}" -n '^\s*fn bundle_digest_equals_sha256_of_written_bytes\(' crates/assay-core/src/replay/bundle/tests.rs >/dev/null
"${rg_bin}" -n '^\s*fn entries_written_in_sorted_order\(' crates/assay-core/src/replay/bundle/tests.rs >/dev/null
"${rg_bin}" -n '^\s*fn validate_entry_path_accepts_valid_paths\(' crates/assay-core/src/replay/bundle/tests.rs >/dev/null

echo 'Replay bundle Step2 reviewer script: PASS'
