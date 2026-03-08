#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-CHECKLIST-coverage-command-step3.md"
  "docs/contributing/SPLIT-REVIEW-PACK-coverage-command-step3.md"
  "scripts/ci/review-coverage-command-step3.sh"
)

echo "[review] step3 docs+script-only diff vs $BASE_REF (workflow-ban)"

while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave19 Step3 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave19 Step3: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] rerun Step2 invariants"

FACADE=""
if [[ -f "crates/assay-cli/src/cli/commands/coverage/mod.rs" ]]; then
  FACADE="crates/assay-cli/src/cli/commands/coverage/mod.rs"
elif [[ -f "crates/assay-cli/src/cli/commands/coverage.rs" ]]; then
  FACADE="crates/assay-cli/src/cli/commands/coverage.rs"
else
  echo "FAIL: neither coverage/mod.rs nor coverage.rs exists"
  exit 1
fi

echo "[review] facade invariants: $FACADE"

FACADE_LOC="$(python3 - <<'PY'
from pathlib import Path
p = Path("crates/assay-cli/src/cli/commands/coverage/mod.rs")
if not p.exists():
    p = Path("crates/assay-cli/src/cli/commands/coverage.rs")
print(sum(1 for line in p.read_text(encoding="utf-8").splitlines() if line.strip()))
PY
)"
if [[ "$FACADE_LOC" -gt 200 ]]; then
  echo "FAIL: facade LOC budget exceeded ($FACADE_LOC > 200): $FACADE"
  exit 1
fi

if [[ "$FACADE" == "crates/assay-cli/src/cli/commands/coverage/mod.rs" ]]; then
  rg -n '^mod generate;\s*$' "$FACADE" >/dev/null || { echo "FAIL: missing 'mod generate;'"; exit 1; }
  rg -n '^mod legacy;\s*$' "$FACADE" >/dev/null || { echo "FAIL: missing 'mod legacy;'"; exit 1; }
  rg -n '^mod io;\s*$' "$FACADE" >/dev/null || { echo "FAIL: missing 'mod io;'"; exit 1; }
fi

rg -n 'pub\s+async\s+fn\s+cmd_coverage|pub\s+fn\s+cmd_coverage' "$FACADE" >/dev/null || {
  echo "FAIL: facade must expose cmd_coverage(...)"
  exit 1
}

# Keep direct file-write logic out of facade.
if rg -n 'create_dir_all|tokio::fs::write' "$FACADE" >/dev/null; then
  echo "FAIL: facade must not contain direct file-write logic"
  rg -n 'create_dir_all|tokio::fs::write' "$FACADE"
  exit 1
fi

if rg -n 'export_baseline|min_coverage|baseline_failed|threshold_failed' "$FACADE" >/dev/null; then
  echo "FAIL: facade must not contain legacy baseline/threshold logic"
  rg -n 'export_baseline|min_coverage|baseline_failed|threshold_failed' "$FACADE"
  exit 1
fi

echo "[review] generate path markers"
[[ -f "crates/assay-cli/src/cli/commands/coverage/generate.rs" ]] || {
  echo "FAIL: missing generate.rs"
  exit 1
}
rg -n 'out_md|routes_top|declared_tools_file|declared_tools' \
  crates/assay-cli/src/cli/commands/coverage/generate.rs >/dev/null || {
  echo "FAIL: generate.rs missing generator-mode markers"
  exit 1
}

echo "[review] legacy path markers"
[[ -f "crates/assay-cli/src/cli/commands/coverage/legacy.rs" ]] || {
  echo "FAIL: missing legacy.rs"
  exit 1
}
rg -n 'baseline|min_coverage|threshold_failed|baseline_failed|CoverageAnalyzer|Baseline::' \
  crates/assay-cli/src/cli/commands/coverage/legacy.rs >/dev/null || {
  echo "FAIL: legacy.rs missing baseline/analyzer markers"
  exit 1
}

echo "[review] io path markers"
[[ -f "crates/assay-cli/src/cli/commands/coverage/io.rs" ]] || {
  echo "FAIL: missing io.rs"
  exit 1
}
rg -n 'tokio::fs::write|create_dir_all|Wrote coverage_report_v1|Wrote coverage_report_v1 markdown' \
  crates/assay-cli/src/cli/commands/coverage/io.rs >/dev/null || {
  echo "FAIL: io.rs missing write/logging markers"
  exit 1
}

if rg -n 'validate_coverage_report_v1|build_coverage_report_from_input|CoverageAnalyzer::from_policy|Baseline::from_coverage_report' \
  crates/assay-cli/src/cli/commands/coverage/io.rs >/dev/null; then
  echo "FAIL: io.rs must not contain schema/report-build/legacy analyzer logic"
  rg -n 'validate_coverage_report_v1|build_coverage_report_from_input|CoverageAnalyzer::from_policy|Baseline::from_coverage_report' \
    crates/assay-cli/src/cli/commands/coverage/io.rs
  exit 1
fi

echo "[review] build + tests"
cargo fmt --check
cargo clippy -p assay-cli --all-targets -- -D warnings

cargo test -p assay-cli coverage_contract
cargo test -p assay-cli coverage_out_md
cargo test -p assay-cli coverage_declared_tools_file

echo "[review] PASS"
