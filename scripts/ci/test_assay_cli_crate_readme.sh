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
  escaped-label-relative
  nested-label-relative
  autolink-mutable
  html-unquoted-relative
  html-unquoted-mutable
  html-entity-mutable
  srcset-relative
  blob-HEAD
  blob-main
  blob-refs-heads-main
  workspace-readme-fallback
  version-pinned-install
  version-pinned-package-id
  version-pinned-option-first
  version-pinned-version-first
  package-grew-docs
  green-control
  hostile-bracket-bound
  scanner-structural-bound
  oversize-readme
  oversize-archive-member
  packaged-manifest-source
  toolchain-single-source
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


def measure(n: int) -> float:
    text = "[" * n
    started = time.perf_counter()
    try:
        extract(text)
    except SystemExit:
        return time.perf_counter() - started
    raise SystemExit("hostile unmatched-label input did not fail closed")


elapsed = measure(1024 * 1024)
links = extract("[ok](evidence_demo_profile.yaml)")
print(f"hostile-bracket-bound 1MiB={elapsed:.4f}s")
if elapsed > 1.0:
    raise SystemExit(f"extract_links did not reject hostile input promptly: {elapsed:.4f}s")
if "evidence_demo_profile.yaml" not in links:
    raise SystemExit(f"strict scanner hid the real packaged link: {links!r}")
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

write_fake_cargo() {
  local destination="$1"
  local oversized="${2:-false}"
  cat >"$destination" <<SH
#!/usr/bin/env bash
set -euo pipefail
mkdir -p "\$CARGO_TARGET_DIR/package"
python3 - "\$CARGO_TARGET_DIR/package/assay-cli-5.5.0.crate" \\
  "\$FAKE_MANIFEST" "\$FAKE_README" "\$FAKE_PROFILE" "$oversized" <<'PY'
from pathlib import Path
import gzip
import io
import sys
import tarfile

files = {
    "Cargo.toml.orig": Path(sys.argv[2]).read_bytes(),
    "README.md": Path(sys.argv[3]).read_bytes(),
    "evidence_demo_profile.yaml": Path(sys.argv[4]).read_bytes(),
}
if sys.argv[5] == "true":
    # A valid gzip stream with a deliberately truncated tar payload is enough:
    # the checker must reject the declared size as soon as it reads the header.
    with gzip.open(sys.argv[1], "wb") as stream:
        for relative, data in files.items():
            info = tarfile.TarInfo(f"assay-cli-5.5.0/{relative}")
            info.size = len(data)
            stream.write(info.tobuf())
            stream.write(data)
            stream.write(b"\0" * ((-len(data)) % 512))
        oversized = tarfile.TarInfo("assay-cli-5.5.0/src/oversized.bin")
        oversized.size = 64 * 1024 * 1024 + 1
        stream.write(oversized.tobuf())
else:
    with tarfile.open(sys.argv[1], "w:gz") as archive:
        for relative, data in files.items():
            info = tarfile.TarInfo(f"assay-cli-5.5.0/{relative}")
            info.size = len(data)
            archive.addfile(info, io.BytesIO(data))
PY
SH
  chmod +x "$destination"
}

run_packaged_manifest_source() {
  local fakebin="$SCRATCH/packaged-manifest-source-bin"
  local out="$SCRATCH/packaged-manifest-source.out"
  mkdir -p "$fakebin"
  write_fake_cargo "$fakebin/cargo"

  python3 - "$MANIFEST" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
old = 'readme = "README.md"'
if text.count(old) != 1:
    raise SystemExit("expected one crate-owned readme assignment")
path.write_text(text.replace(old, "readme.workspace = true", 1), encoding="utf-8")
PY

  if ! PATH="$fakebin:$PATH" \
    FAKE_MANIFEST="$SCRATCH/Cargo.toml" \
    FAKE_README="$SCRATCH/README.md" \
    FAKE_PROFILE="$ROOT/crates/assay-cli/evidence_demo_profile.yaml" \
    bash "$CHECK" >"$out" 2>&1; then
    echo "FAIL: packaged manifest did not override disagreeing checkout manifest" >&2
    cat "$out" >&2
    return 1
  fi
}

run_oversize_archive_member() {
  local fakebin="$SCRATCH/oversize-archive-bin"
  local out="$SCRATCH/oversize-archive.out"
  mkdir -p "$fakebin"
  write_fake_cargo "$fakebin/cargo" true
  if PATH="$fakebin:$PATH" \
    FAKE_MANIFEST="$SCRATCH/Cargo.toml" \
    FAKE_README="$SCRATCH/README.md" \
    FAKE_PROFILE="$ROOT/crates/assay-cli/evidence_demo_profile.yaml" \
    bash "$CHECK" >"$out" 2>&1; then
    echo "FAIL: oversized declared archive member was accepted" >&2
    return 1
  fi
  if ! grep -Fq "declared uncompressed size exceeds" "$out"; then
    echo "FAIL: oversized archive member missed size diagnostic" >&2
    cat "$out" >&2
    return 1
  fi
}

run_toolchain_single_source() {
  python3 - "$ROOT/.github/workflows/ci.yml" <<'PY'
from pathlib import Path
import sys

workflow = Path(sys.argv[1]).read_text(encoding="utf-8")
start = workflow.index("  publish-shape-cli:")
end = workflow.index("\n  public-crate-policy:", start)
job = workflow[start:end]
if job.count("RUSTUP_TOOLCHAIN: stable") != 1:
    raise SystemExit("publish-shape toolchain must have one literal source")
if "toolchain: ${{ env.RUSTUP_TOOLCHAIN }}" not in job:
    raise SystemExit("rust-toolchain action does not consume the job toolchain pin")
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
      expect_fail "$name" "unsupported link syntax"
      ;;
    escaped-label-relative)
      printf '\n[x\\]](docs/guides/github-action.md)\n' >>"$README"
      expect_fail "$name" "unsupported link syntax"
      ;;
    nested-label-relative)
      printf '\n[a [b]](docs/guides/github-action.md)\n' >>"$README"
      expect_fail "$name" "unsupported link syntax"
      ;;
    autolink-mutable)
      printf '\n<https://github.com/Rul1an/assay/blob/main/README.md>\n' >>"$README"
      expect_fail "$name" "unsupported link syntax"
      ;;
    html-unquoted-relative)
      printf '\n<a href=docs/guides/github-action.md>missing</a>\n' >>"$README"
      expect_fail "$name" "unsupported link syntax"
      ;;
    html-unquoted-mutable)
      printf '\n<img src=https://github.com/Rul1an/assay/blob/refs/heads/main/x.png>\n' >>"$README"
      expect_fail "$name" "unsupported link syntax"
      ;;
    html-entity-mutable)
      printf '\n[x](https://github.com/Rul1an/assay/blob&#47;main/README.md)\n' >>"$README"
      expect_fail "$name" "unsupported link syntax"
      ;;
    srcset-relative)
      printf '\n<img srcset="evidence_demo_profile.yaml 1x, docs/missing.png 2x">\n' >>"$README"
      expect_fail "$name" "unsupported link syntax"
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
    version-pinned-option-first)
      rewrite_install "cargo install --locked assay-cli@5.4.0"
      expect_fail "$name" "version pin"
      ;;
    version-pinned-version-first)
      rewrite_install "cargo install --version 5.4.0 assay-cli --locked"
      expect_fail "$name" "version pin"
      ;;
    package-grew-docs)
      create_docs_sentinel
      expect_fail "$name" "forbidden prefix"
      ;;
    green-control)
      printf '\nGreen control: prose-only no-op.\n' >>"$README"
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
    oversize-archive-member)
      run_oversize_archive_member
      echo "PASS: $name"
      ;;
    packaged-manifest-source)
      run_packaged_manifest_source
      echo "PASS: $name"
      ;;
    toolchain-single-source)
      run_toolchain_single_source
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
