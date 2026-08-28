#!/usr/bin/env bash
# Named mutations for scripts/ci/check_assay_cli_crate_readme.sh.
# Scratch mutations against the real crate files; restore clean on exit.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

CHECK="$ROOT/scripts/ci/check_assay_cli_crate_readme.sh"
MANIFEST="$ROOT/crates/assay-cli/Cargo.toml"
README="$ROOT/crates/assay-cli/README.md"
SELECTED_CASE="${ASSAY_CLI_CRATE_README_CASE:-}"

CASES=(
  relative-unpackaged-docs
  blob-HEAD
  blob-main
  workspace-readme-fallback
  version-pinned-install
  package-grew-docs
  green-control
)
EXPECTED_CASES="${#CASES[@]}"

echo "collected cases:"
printf '%s\n' "${CASES[@]}"

if [[ -n "$SELECTED_CASE" ]]; then
  known=false
  for name in "${CASES[@]}"; do
    if [[ "$name" == "$SELECTED_CASE" ]]; then
      known=true
      break
    fi
  done
  if [[ "$known" != true ]]; then
    echo "FAIL: unknown case filter: $SELECTED_CASE" >&2
    exit 1
  fi
fi

if [[ ! -f "$README" ]]; then
  echo "FAIL: crate README is missing; mutations require the GREEN tree" >&2
  exit 1
fi

SCRATCH="$(mktemp -d)"
trap 'restore_tree; rm -rf "$SCRATCH"' EXIT
cp "$MANIFEST" "$SCRATCH/Cargo.toml"
cp "$README" "$SCRATCH/README.md"

restore_tree() {
  cp "$SCRATCH/Cargo.toml" "$MANIFEST"
  cp "$SCRATCH/README.md" "$README"
  rm -rf "$ROOT/crates/assay-cli/docs"
}

run_check() {
  local out="$1"
  bash "$CHECK" >"$out" 2>&1
}

expect_fail() {
  local name="$1"
  local needle="$2"
  local out="$SCRATCH/$name.out"
  if run_check "$out"; then
    echo "FAIL: mutation $name was not observed" >&2
    cat "$out" >&2
    return 1
  fi
  if ! grep -Fq "$needle" "$out"; then
    echo "FAIL: mutation $name missed diagnostic: $needle" >&2
    cat "$out" >&2
    return 1
  fi
  echo "PASS: $name"
}

expect_pass() {
  local name="$1"
  local out="$SCRATCH/$name.out"
  if ! run_check "$out"; then
    echo "FAIL: mutation $name must stay green" >&2
    cat "$out" >&2
    return 1
  fi
  echo "PASS: $name"
}

baseline_out="$SCRATCH/baseline.out"
if ! run_check "$baseline_out"; then
  echo "FAIL: baseline must be green before named mutations" >&2
  cat "$baseline_out" >&2
  exit 1
fi

mutation_count=0
for name in "${CASES[@]}"; do
  if [[ -n "$SELECTED_CASE" && "$name" != "$SELECTED_CASE" ]]; then
    continue
  fi
  restore_tree
  case "$name" in
    relative-unpackaged-docs)
      printf '\n[x](docs/guides/github-action.md)\n' >>"$README"
      expect_fail "$name" "member-list miss"
      ;;
    blob-HEAD)
      python3 - "$README" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
old = "https://github.com/Rul1an/assay"
new = "https://github.com/Rul1an/assay/blob/HEAD/"
if text.count(old) != 1:
    raise SystemExit(f"expected one repo root link, found {text.count(old)}")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
PY
      expect_fail "$name" "mutable git ref"
      ;;
    blob-main)
      python3 - "$README" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
old = "https://github.com/Rul1an/assay"
new = "https://github.com/Rul1an/assay/blob/main/"
if text.count(old) != 1:
    raise SystemExit(f"expected one repo root link, found {text.count(old)}")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
PY
      expect_fail "$name" "mutable git ref"
      ;;
    workspace-readme-fallback)
      python3 - "$MANIFEST" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
old = 'readme = "README.md"'
new = "readme.workspace = true"
if text.count(old) != 1:
    raise SystemExit("expected one crate-owned readme assignment")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
PY
      expect_fail "$name" "not crate-owned README"
      ;;
    version-pinned-install)
      python3 - "$README" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
old = "cargo install assay-cli --locked"
new = "cargo install assay-cli --version 5.4.0 --locked"
if text.count(old) != 1:
    raise SystemExit("expected one unpinned cargo install command")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
PY
      expect_fail "$name" "version pin"
      ;;
    package-grew-docs)
      mkdir -p "$ROOT/crates/assay-cli/docs/guides"
      printf '%s\n' "appease" >"$ROOT/crates/assay-cli/docs/guides/github-action.md"
      expect_fail "$name" "forbidden prefix"
      ;;
    green-control)
      printf '\n<!-- green-control: comment-only no-op -->\n' >>"$README"
      expect_pass "$name"
      ;;
    *)
      echo "FAIL: unhandled case $name" >&2
      exit 1
      ;;
  esac
  mutation_count=$((mutation_count + 1))
done

restore_tree

if [[ -n "$SELECTED_CASE" ]]; then
  if [[ "$mutation_count" -ne 1 ]]; then
    echo "FAIL: selected case executed $mutation_count, wanted 1" >&2
    exit 1
  fi
elif [[ "$mutation_count" -ne "$EXPECTED_CASES" ]]; then
  echo "FAIL: mutation_count=$mutation_count expected=$EXPECTED_CASES" >&2
  exit 1
fi

echo "mutation_count=$mutation_count expected=$EXPECTED_CASES"
echo "assay-cli crate README mutations OK"
