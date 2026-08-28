#!/usr/bin/env bash
# Named mutations for scripts/ci/check_assay_cli_crate_readme.sh.
# Scratch mutations against the real crate files; restore only what this
# script created. Do not blanket-delete crates/assay-cli/docs.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

CHECK="$ROOT/scripts/ci/check_assay_cli_crate_readme.sh"
MANIFEST="$ROOT/crates/assay-cli/Cargo.toml"
README="$ROOT/crates/assay-cli/README.md"
DOCS_ROOT="$ROOT/crates/assay-cli/docs"
SELECTED_CASE="${ASSAY_CLI_CRATE_README_CASE:-}"

CASES=(
  relative-unpackaged-docs
  reference-unpackaged-docs
  html-unquoted-relative
  html-unquoted-mutable
  blob-HEAD
  blob-main
  blob-refs-heads-main
  workspace-readme-fallback
  version-pinned-install
  version-pinned-package-id
  package-grew-docs
  green-control
  hostile-bracket-bound
  scanner-structural-bound
  oversize-readme
  packaged-manifest-source
  restore-preexisting-docs
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

if [[ -e "$DOCS_ROOT" ]]; then
  echo "FAIL: $DOCS_ROOT already exists; refusing to mutate" >&2
  exit 1
fi

SCRATCH="$(mktemp -d)"
CREATED_PATHS=()
trap 'restore_tree; rm -rf "$SCRATCH"' EXIT
cp "$MANIFEST" "$SCRATCH/Cargo.toml"
cp "$README" "$SCRATCH/README.md"

restore_owned_paths() {
  local p
  local sorted
  if [[ "${#CREATED_PATHS[@]}" -eq 0 ]]; then
    return 0
  fi
  # Deepest owned paths first so files go before their parent dirs.
  while IFS= read -r p; do
    sorted+=("$p")
  done < <(printf '%s\n' "${CREATED_PATHS[@]}" | awk '{ print length, $0 }' | sort -nr | cut -d' ' -f2-)
  for p in "${sorted[@]}"; do
    if [[ -f "$p" ]]; then
      rm -f "$p"
    elif [[ -d "$p" ]]; then
      rmdir "$p" 2>/dev/null || {
        echo "FAIL: owned directory not empty, not blanket-deleting: $p" >&2
        return 1
      }
    fi
  done
  CREATED_PATHS=()
}

restore_tree() {
  cp "$SCRATCH/Cargo.toml" "$MANIFEST"
  cp "$SCRATCH/README.md" "$README"
  restore_owned_paths
}

create_docs_sentinel() {
  mkdir -p "$DOCS_ROOT/guides"
  printf '%s\n' "appease" >"$DOCS_ROOT/guides/github-action.md"
  CREATED_PATHS+=(
    "$DOCS_ROOT/guides/github-action.md"
    "$DOCS_ROOT/guides"
    "$DOCS_ROOT"
  )
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

rewrite_repo_link() {
  local new="$1"
  python3 - "$README" "$new" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
new = sys.argv[2]
text = path.read_text(encoding="utf-8")
old = "https://github.com/Rul1an/assay"
if text.count(old) != 1:
    raise SystemExit(f"expected one repo root link, found {text.count(old)}")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
PY
}

rewrite_install() {
  local new="$1"
  python3 - "$README" "$new" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
new = sys.argv[2]
text = path.read_text(encoding="utf-8")
old = "cargo install assay-cli --locked"
if text.count(old) != 1:
    raise SystemExit("expected one unpinned cargo install command")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
PY
}

run_hostile_bracket_bound() {
  python3 - "$CHECK" <<'PY'
from pathlib import Path
import re
import sys
import tarfile
import time

checker = Path(sys.argv[1]).read_text(encoding="utf-8")
if "MD_LINK = re.compile" in checker:
    raise SystemExit("polynomial MD_LINK regex still present")

# Exec only the extract helper from the crate checker heredoc, not the cargo gate.
start = checker.index("MAX_LINK_LABEL")
end = checker.index("\ndef classify_relative")
ns: dict = {"Path": Path, "re": re, "sys": sys, "tarfile": tarfile}
exec(checker[start:end], ns)
extract = ns["extract_links"]
if "MD_LINK" in ns:
    raise SystemExit("polynomial MD_LINK regex still bound in extract_links")


def measure(n: int) -> tuple[float, list[str]]:
    text = "[" * n + "\n[ok](evidence_demo_profile.yaml)\n"
    started = time.perf_counter()
    links = extract(text)
    return time.perf_counter() - started, links


small_n, large_n = 4000, 16000
t_small, _ = measure(small_n)
t_large, links = measure(large_n)
ratio = t_large / t_small if t_small > 0 else float("inf")
print(f"hostile-bracket-bound 4k={t_small:.4f}s 16k={t_large:.4f}s ratio={ratio:.2f}")
# 16k is 4x 4k. Linear is ~4x; the old MD_LINK was ~4x per doubling (~16x here).
if ratio > 8:
    raise SystemExit(
        f"extract_links is not linear/bounded: 4k={t_small:.4f}s 16k={t_large:.4f}s ratio={ratio:.2f}"
    )
if "evidence_demo_profile.yaml" not in links:
    raise SystemExit(f"hostile prefix hid the real packaged link: {links!r}")
PY
}

run_scanner_structural_bound() {
  python3 - "$CHECK" <<'PY'
from pathlib import Path
import sys

checker = Path(sys.argv[1]).read_text(encoding="utf-8")
if "source[index:]" in checker or "source[index :]" in checker:
    raise SystemExit("unbounded suffix copy remains in scanner")
if "MAX_README_BYTES" not in checker or "read_bounded_utf8" not in checker:
    raise SystemExit("README is materialized without an explicit byte ceiling")
PY
}

run_packaged_manifest_source() {
  python3 - "$CHECK" <<'PY'
from pathlib import Path
import sys

checker = Path(sys.argv[1]).read_text(encoding="utf-8")
required = ("cargo package -p assay-cli --no-verify", "tarfile.open", "load_packaged_crate")
missing = [token for token in required if token not in checker]
if missing:
    raise SystemExit(f"checker does not derive README from built crate: {missing}")
for forbidden in (
    'ROOT / "crates" / "assay-cli" / "Cargo.toml"',
    "cargo metadata --format-version",
):
    if forbidden in checker:
        raise SystemExit(f"checkout metadata remains authoritative: {forbidden}")
PY
}

run_restore_preexisting_docs() {
  python3 - "$SCRATCH" <<'PY'
from pathlib import Path
import shutil
import sys

scratch = Path(sys.argv[1]) / "restore-preexisting-docs"
old = scratch / "old-restore"
new = scratch / "new-restore"
for root in (old, new):
    docs = root / "docs"
    docs.mkdir(parents=True)
    (docs / "user-owned.md").write_text("keep\n", encoding="utf-8")

# Old contract: unconditional rm -rf docs.
shutil.rmtree(old / "docs")
if (old / "docs" / "user-owned.md").exists():
    raise SystemExit("old-restore fixture did not model blanket delete")

# New contract: restore only an owned sentinel; preexisting docs stay.
owned = new / "docs" / "guides" / "github-action.md"
owned.parent.mkdir(parents=True)
owned.write_text("appease\n", encoding="utf-8")
owned.unlink()
owned.parent.rmdir()
# do not rmdir docs — it still holds user-owned.md
if not (new / "docs" / "user-owned.md").exists():
    raise SystemExit("new restore destroyed a preexisting docs file")
if (new / "docs" / "guides").exists():
    raise SystemExit("owned sentinel directory was not removed")
print("restore-preexisting-docs: old rm -rf destroyed user-owned.md; new restore kept it")
PY
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
    reference-unpackaged-docs)
      printf '\n[x][missing]\n\n[missing]: docs/guides/github-action.md\n' >>"$README"
      expect_fail "$name" "member-list miss"
      ;;
    html-unquoted-relative)
      printf '\n<a href=docs/guides/github-action.md>missing</a>\n' >>"$README"
      expect_fail "$name" "member-list miss"
      ;;
    html-unquoted-mutable)
      printf '\n<img src=https://github.com/Rul1an/assay/blob/refs/heads/main/x.png>\n' >>"$README"
      expect_fail "$name" "mutable git ref"
      ;;
    blob-HEAD)
      rewrite_repo_link "https://github.com/Rul1an/assay/blob/HEAD/"
      expect_fail "$name" "mutable git ref"
      ;;
    blob-main)
      rewrite_repo_link "https://github.com/Rul1an/assay/blob/main/"
      expect_fail "$name" "mutable git ref"
      ;;
    blob-refs-heads-main)
      rewrite_repo_link "https://github.com/Rul1an/assay/blob/refs/heads/main/"
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
      rewrite_install "cargo install assay-cli --version 5.4.0 --locked"
      expect_fail "$name" "version pin"
      ;;
    version-pinned-package-id)
      rewrite_install "cargo install assay-cli@5.4.0 --locked"
      expect_fail "$name" "version pin"
      ;;
    package-grew-docs)
      create_docs_sentinel
      expect_fail "$name" "forbidden prefix"
      ;;
    green-control)
      printf '\n<!-- green-control: comment-only no-op -->\n' >>"$README"
      expect_pass "$name"
      ;;
    hostile-bracket-bound)
      run_hostile_bracket_bound
      echo "PASS: $name"
      ;;
    scanner-structural-bound)
      run_scanner_structural_bound
      echo "PASS: $name"
      ;;
    oversize-readme)
      python3 - "$README" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
with path.open("a", encoding="utf-8") as stream:
    stream.write("x" * (1024 * 1024))
PY
      expect_fail "$name" "exceeds 1048576 bytes"
      ;;
    packaged-manifest-source)
      run_packaged_manifest_source
      echo "PASS: $name"
      ;;
    restore-preexisting-docs)
      run_restore_preexisting_docs
      echo "PASS: $name"
      ;;
    *)
      echo "FAIL: unhandled case $name" >&2
      exit 1
      ;;
  esac
  mutation_count=$((mutation_count + 1))
done

restore_tree

if [[ -e "$DOCS_ROOT" ]]; then
  echo "FAIL: docs sentinel leaked: $DOCS_ROOT" >&2
  exit 1
fi

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
