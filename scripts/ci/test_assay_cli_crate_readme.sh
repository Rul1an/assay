#!/usr/bin/env bash
# Named mutations for scripts/ci/check_assay_cli_crate_readme.sh.
# Scratch mutations against the real crate files plus, for unpublished
# workspace-dep fixtures, root Cargo.toml / Cargo.lock / the checker.
# Restore only what this script created. Do not blanket-delete
# crates/assay-cli/docs. Do not git checkout -- the whole tree.
# Unpublished fixtures rewrite assay-* path+version workspace.dependencies
# to the never-published sentinel 0.0.0-unpublished (any current
# workspace.package version). Do not require a successor crates.io version.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

CHECK="$ROOT/scripts/ci/check_assay_cli_crate_readme.sh"
MANIFEST="$ROOT/crates/assay-cli/Cargo.toml"
README="$ROOT/crates/assay-cli/README.md"
ROOT_MANIFEST="$ROOT/Cargo.toml"
ROOT_LOCK="$ROOT/Cargo.lock"
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
  scheme-relative-external
  blob-HEAD
  blob-main
  blob-main-fragment
  blob-main-query
  blob-feature-branch
  blob-other-repo-main
  blob-userinfo-main
  blob-port-main
  blob-scheme-relative-main
  blob-www-main
  blob-trailing-dot-main
  blob-commit
  blob-refs-heads-main
  raw-github-main
  raw-github-commit
  github-raw-main
  github-raw-commit
  archive-main
  archive-commit
  codeload-main
  codeload-commit
  codeload-legacy-main
  codeload-legacy-commit
  codeload-unknown-route
  workspace-readme-fallback
  version-pinned-install
  version-pinned-package-id
  version-pinned-option-first
  version-pinned-version-first
  version-pinned-shell-prompt
  version-pinned-env-prefix
  version-pinned-sudo-prefix
  version-pinned-command-prefix
  version-pinned-path-cargo
  version-pinned-inline-code
  version-unpinned-followed-by-cargo-version
  install-command-missing
  install-command-wrong-package
  install-command-unlocked
  package-grew-docs
  green-control
  hostile-bracket-bound
  scanner-structural-bound
  oversize-readme
  oversize-archive-member
  oversized-gnu-longname
  packaged-manifest-source
  missing-consumer-manifest
  disagreeing-consumer-manifest
  unpublished-workspace-dep
  unpublished-workspace-dep-requires-exclude-lockfile
  toolchain-single-source
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
REPLAY_SCRATCHES=()
cleanup_all() {
  restore_tree
  rm -rf "$SCRATCH"
  local d
  for d in "${REPLAY_SCRATCHES[@]+"${REPLAY_SCRATCHES[@]}"}"; do
    rm -rf "$d" "${d}.tar"
  done
}
trap cleanup_all EXIT
cp "$MANIFEST" "$SCRATCH/Cargo.toml"
cp "$README" "$SCRATCH/README.md"
cp "$ROOT_MANIFEST" "$SCRATCH/root-Cargo.toml"
cp "$ROOT_LOCK" "$SCRATCH/Cargo.lock"
cp "$CHECK" "$SCRATCH/check_assay_cli_crate_readme.sh"

restore_tree() {
  cp "$SCRATCH/Cargo.toml" "$MANIFEST"
  cp "$SCRATCH/README.md" "$README"
  cp "$SCRATCH/root-Cargo.toml" "$ROOT_MANIFEST"
  cp "$SCRATCH/Cargo.lock" "$ROOT_LOCK"
  cp "$SCRATCH/check_assay_cli_crate_readme.sh" "$CHECK"
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
import gzip
import re
import sys
import tarfile
import time
from urllib.parse import urlsplit

checker = Path(sys.argv[1]).read_text(encoding="utf-8")
if "MD_LINK = re.compile" in checker:
    raise SystemExit("polynomial MD_LINK regex still present")

# Exec only the extract helper from the crate checker heredoc, not the cargo gate.
start = checker.index("MAX_LINK_LABEL")
end = checker.index("\ndef classify_relative")
ns: dict = {
    "Path": Path,
    "gzip": gzip,
    "re": re,
    "sys": sys,
    "tarfile": tarfile,
    "urlsplit": urlsplit,
}
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
  local mode="${2:-normal}"
  cat >"$destination" <<SH
#!/usr/bin/env bash
set -euo pipefail
mkdir -p "\$CARGO_TARGET_DIR/package"
python3 - "\$CARGO_TARGET_DIR/package/assay-cli-5.5.0.crate" \\
  "\$FAKE_MANIFEST" "\$FAKE_README" "\$FAKE_PROFILE" "$mode" <<'PY'
from pathlib import Path
import gzip
import io
import sys
import tarfile

files = {
    "Cargo.toml": Path(sys.argv[2]).read_bytes(),
    "Cargo.toml.orig": Path(sys.argv[2]).read_bytes(),
    "README.md": Path(sys.argv[3]).read_bytes(),
    "evidence_demo_profile.yaml": Path(sys.argv[4]).read_bytes(),
}
if sys.argv[5] == "missing-consumer-manifest":
    del files["Cargo.toml"]
elif sys.argv[5] == "disagreeing-consumer-manifest":
    files["Cargo.toml"] = files["Cargo.toml"].replace(
        b'readme = "README.md"', b"readme.workspace = true", 1
    )
if sys.argv[5] == "forbidden-doc":
    files["docs/guides/github-action.md"] = b"must not ship\n"
if sys.argv[5] == "oversized-member":
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
elif sys.argv[5] == "gnu-longname":
    # GNU longname metadata is consumed by tarfile before it yields the next
    # member. The checker must cap the decompressed stream around tarfile, not
    # merely sum the regular members tarfile eventually yields.
    with gzip.open(sys.argv[1], "wb") as stream:
        longname = tarfile.TarInfo("././@LongLink")
        longname.type = tarfile.GNUTYPE_LONGNAME
        longname.size = 16 * 1024 * 1024 + 1
        stream.write(longname.tobuf(format=tarfile.GNU_FORMAT))
        remaining = longname.size
        chunk = b"\0" * (1024 * 1024)
        while remaining:
            part = chunk[: min(remaining, len(chunk))]
            stream.write(part)
            remaining -= len(part)
        stream.write(b"\0" * ((-longname.size) % 512))
        stream.write(b"\0" * 1024)
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

run_consumer_manifest_failure() {
  local mode="$1"
  local needle="$2"
  local fakebin="$SCRATCH/$mode-bin"
  local out="$SCRATCH/$mode.out"
  mkdir -p "$fakebin"
  write_fake_cargo "$fakebin/cargo" "$mode"
  if PATH="$fakebin:$PATH" \
    FAKE_MANIFEST="$SCRATCH/Cargo.toml" \
    FAKE_README="$SCRATCH/README.md" \
    FAKE_PROFILE="$ROOT/crates/assay-cli/evidence_demo_profile.yaml" \
    bash "$CHECK" >"$out" 2>&1; then
    echo "FAIL: $mode was accepted" >&2
    return 1
  fi
  if ! grep -Fq "$needle" "$out"; then
    echo "FAIL: $mode missed diagnostic: $needle" >&2
    cat "$out" >&2
    return 1
  fi
}

run_oversize_archive_member() {
  local fakebin="$SCRATCH/oversize-archive-bin"
  local out="$SCRATCH/oversize-archive.out"
  mkdir -p "$fakebin"
  write_fake_cargo "$fakebin/cargo" oversized-member
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

run_forbidden_package_member() {
  local fakebin="$SCRATCH/forbidden-package-member-bin"
  local out="$SCRATCH/forbidden-package-member.out"
  mkdir -p "$fakebin"
  write_fake_cargo "$fakebin/cargo" forbidden-doc
  if PATH="$fakebin:$PATH" \
    FAKE_MANIFEST="$SCRATCH/Cargo.toml" \
    FAKE_README="$SCRATCH/README.md" \
    FAKE_PROFILE="$ROOT/crates/assay-cli/evidence_demo_profile.yaml" \
    bash "$CHECK" >"$out" 2>&1; then
    echo "FAIL: forbidden docs member was accepted" >&2
    return 1
  fi
  if ! grep -Fq "forbidden prefix" "$out"; then
    echo "FAIL: forbidden docs member missed prefix diagnostic" >&2
    cat "$out" >&2
    return 1
  fi
}

run_oversized_gnu_longname() {
  local fakebin="$SCRATCH/oversized-gnu-longname-bin"
  local out="$SCRATCH/oversized-gnu-longname.out"
  mkdir -p "$fakebin"
  write_fake_cargo "$fakebin/cargo" gnu-longname
  if PATH="$fakebin:$PATH" \
    FAKE_MANIFEST="$SCRATCH/Cargo.toml" \
    FAKE_README="$SCRATCH/README.md" \
    FAKE_PROFILE="$ROOT/crates/assay-cli/evidence_demo_profile.yaml" \
    bash "$CHECK" >"$out" 2>&1; then
    echo "FAIL: oversized GNU longname metadata was accepted" >&2
    return 1
  fi
  if ! grep -Fq "decompressed stream exceeds" "$out"; then
    echo "FAIL: oversized GNU longname missed stream diagnostic" >&2
    cat "$out" >&2
    return 1
  fi
}

run_consumer_replay() {
  if [[ -n "$SELECTED_CASE" ]]; then
    return 0
  fi
  if [[ "${ASSAY_CLI_CRATE_README_REPLAY:-}" == "1" ]]; then
    return 0
  fi

  local rc_sha="31335cc3a2c33c2621fd1cbf100e48e07dedbea6"
  local main_sha=""
  if git -C "$ROOT" rev-parse --verify origin/main >/dev/null 2>&1; then
    main_sha="$(git -C "$ROOT" rev-parse origin/main)"
  elif git -C "$ROOT" rev-parse --verify refs/remotes/origin/main >/dev/null 2>&1; then
    main_sha="$(git -C "$ROOT" rev-parse refs/remotes/origin/main)"
  else
    main_sha="$(git -C "$ROOT" rev-parse HEAD^)"
  fi
  echo "consumer-replay main_sha=$main_sha rc_sha=$rc_sha"

  materialize_replay_tree() {
    local sha="$1"
    local dest="$2"
    local tarball="${dest}.tar"
    if ! git -C "$ROOT" archive --format=tar -o "$tarball" "$sha" 2>/dev/null; then
      if ! git -C "$ROOT" fetch --no-filter origin "$sha"; then
        echo "FAIL: could not fetch $sha for crate-README consumer replay" >&2
        rm -f "$tarball"
        return 1
      fi
      if ! git -C "$ROOT" archive --format=tar -o "$tarball" "$sha"; then
        echo "FAIL: git archive $sha failed for crate-README consumer replay" >&2
        rm -f "$tarball"
        return 1
      fi
    fi
    mkdir -p "$dest"
    tar -xf "$tarball" -C "$dest"
    rm -f "$tarball"
  }

  replay_one() {
    local label="$1"
    local sha="$2"
    local dest out packaged case_name case_out needle
    dest="$(mktemp -d "/tmp/ruley-cli-crate-readme-replay-${label}-XXXXXX")"
    REPLAY_SCRATCHES+=("$dest")
    echo "consumer-replay $label materializing $sha into $dest"
    if ! materialize_replay_tree "$sha" "$dest"; then
      echo "FAIL: consumer replay $label could not materialize $sha" >&2
      return 1
    fi
    mkdir -p "$dest/scripts/ci"
    cp "$CHECK" "$dest/scripts/ci/check_assay_cli_crate_readme.sh"
    cp "$ROOT/scripts/ci/test_assay_cli_crate_readme.sh" "$dest/scripts/ci/test_assay_cli_crate_readme.sh"
    chmod +x "$dest/scripts/ci/check_assay_cli_crate_readme.sh" \
      "$dest/scripts/ci/test_assay_cli_crate_readme.sh"

    out="$SCRATCH/replay-${label}-unmodified.out"
    if ! (cd "$dest" && bash ./scripts/ci/check_assay_cli_crate_readme.sh >"$out" 2>&1); then
      echo "FAIL: consumer replay $label unmodified checker failed" >&2
      cat "$out" >&2
      return 1
    fi
    for needle in \
      "assay-cli crate-owned README OK" \
      "not installability proof" \
      "not lockfile proof"; do
      if ! grep -Fq "$needle" "$out"; then
        echo "FAIL: consumer replay $label unmodified checker missed: $needle" >&2
        cat "$out" >&2
        return 1
      fi
    done
    packaged="$(grep -E '^[[:space:]]*Packaging assay-cli ' "$out" | head -n 1 || true)"
    echo "consumer-replay $label unmodified OK ${packaged:-}"

    for case_name in unpublished-workspace-dep unpublished-workspace-dep-requires-exclude-lockfile; do
      case_out="$SCRATCH/replay-${label}-${case_name}.out"
      if ! (
        cd "$dest"
        ASSAY_CLI_CRATE_README_CASE="$case_name" \
          ASSAY_CLI_CRATE_README_REPLAY=1 \
          bash ./scripts/ci/test_assay_cli_crate_readme.sh >"$case_out" 2>&1
      ); then
        echo "FAIL: consumer replay $label $case_name" >&2
        cat "$case_out" >&2
        return 1
      fi
      echo "consumer-replay $label $case_name OK"
    done

    rm -rf "$dest" "${dest}.tar"
  }

  # RC pin first so a fetch failure fails the test without skipping.
  replay_one "rc" "$rc_sha"
  replay_one "main" "$main_sha"
}

bump_unpublished_workspace() {
  python3 - "$ROOT_MANIFEST" <<'PY'
from pathlib import Path
import re
import sys

SENTINEL = "0.0.0-unpublished"
path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
pkg = re.search(r"(?ms)^\[workspace\.package\]\s*$(.+?)(?=^\[|\Z)", text)
if not pkg:
    raise SystemExit("could not read [workspace.package] for unpublished fixture")
ver = re.search(r'(?m)^\s*version\s*=\s*"([^"]+)"\s*$', pkg.group(1))
if not ver:
    raise SystemExit("could not read workspace.package version for unpublished fixture")
if ver.group(1) == SENTINEL:
    raise SystemExit("workspace.package version is already the unpublished sentinel")

changed = [0]

def repl_dep(match: re.Match[str]) -> str:
    if match.group(2) == SENTINEL:
        return match.group(0)
    changed[0] += 1
    return f'{match.group(1)}"{SENTINEL}"'

text, _ = re.subn(
    r'(?m)^(assay-[\w-]+ = \{ path = "[^"]+", version = )"([^"]+)"',
    repl_dep,
    text,
)
if changed[0] < 1:
    raise SystemExit(
        "expected assay-* workspace.dependencies path+version to rewrite to unpublished sentinel"
    )
path.write_text(text, encoding="utf-8")
PY
}

strip_exclude_lockfile_from_checker() {
  python3 - "$CHECK" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
token = " --exclude-lockfile"
if token not in text:
    raise SystemExit("checker is missing --exclude-lockfile to strip")
if "cargo package -p assay-cli --exclude-lockfile --no-verify --allow-dirty" not in text:
    raise SystemExit("expected the real cargo package invocation to carry --exclude-lockfile")
path.write_text(text.replace(token, ""), encoding="utf-8")
PY
}

run_unpublished_workspace_dep() {
  local out="$SCRATCH/unpublished-workspace-dep.out"
  bump_unpublished_workspace
  if ! run_check "$out"; then
    echo "FAIL: unpublished-workspace-dep must stay green" >&2
    cat "$out" >&2
    return 1
  fi
  if ! grep -Fq "assay-cli crate-owned README OK" "$out"; then
    echo "FAIL: unpublished-workspace-dep missed packaged README OK" >&2
    cat "$out" >&2
    return 1
  fi
  if ! grep -Fq "evidence_demo_profile.yaml" "$out"; then
    echo "FAIL: unpublished-workspace-dep missed real cargo package member" >&2
    cat "$out" >&2
    return 1
  fi
  if ! grep -Fq "not installability proof" "$out"; then
    echo "FAIL: unpublished-workspace-dep missed installability non-claim" >&2
    cat "$out" >&2
    return 1
  fi
  if ! grep -Fq "not lockfile proof" "$out"; then
    echo "FAIL: unpublished-workspace-dep missed lockfile non-claim" >&2
    cat "$out" >&2
    return 1
  fi
}

run_unpublished_workspace_dep_requires_exclude_lockfile() {
  local name="unpublished-workspace-dep-requires-exclude-lockfile"
  bump_unpublished_workspace
  strip_exclude_lockfile_from_checker
  expect_fail "$name" "failed to select a version"
  if ! grep -Fq "assay-canonical" "$SCRATCH/$name.out"; then
    echo "FAIL: mutation $name missed diagnostic: assay-canonical" >&2
    cat "$SCRATCH/$name.out" >&2
    return 1
  fi
}

run_toolchain_single_source() {
  python3 - "$ROOT/.github/workflows/ci.yml" <<'PY'
from pathlib import Path
import re
import sys

workflow = Path(sys.argv[1]).read_text(encoding="utf-8")
start = workflow.index("  publish-shape-cli:")
end = workflow.index("\n  public-crate-policy:", start)
job = workflow[start:end]


def validate(candidate: str) -> None:
    if candidate.count("RUSTUP_TOOLCHAIN:") != 1 or candidate.count('RUSTUP_TOOLCHAIN: "1.96.0"') != 1:
        raise ValueError("publish-shape toolchain must have one literal source")
    active_toolchain = re.findall(
        r"(?m)^\s+toolchain:\s*\$\{\{\s*env\.RUSTUP_TOOLCHAIN\s*\}\}\s*$",
        candidate,
    )
    if len(active_toolchain) != 1:
        raise ValueError("rust-toolchain action does not consume the job toolchain pin")
    forbidden = ("cargo +", "RUSTUP_TOOLCHAIN=", "rustup run", "rustup override", "rustup default")
    if any(token in candidate for token in forbidden):
        raise ValueError("publish-shape invocation overrides the pinned toolchain")


validate(job)
mutations = {
    "moving-stable": job.replace('RUSTUP_TOOLCHAIN: "1.96.0"', "RUSTUP_TOOLCHAIN: stable", 1),
    "step-env": job + "\n      env:\n        RUSTUP_TOOLCHAIN: nightly\n",
    "cargo-plus": job + "\n      run: cargo +nightly package\n",
    "shell-env": job + "\n      run: RUSTUP_TOOLCHAIN=nightly cargo package\n",
    "rustup-run": job + "\n      run: rustup run nightly cargo package\n",
    "rustup-override": job + "\n      run: rustup override set nightly\n",
    "commented-action-input": job.replace(
        "          toolchain: ${{ env.RUSTUP_TOOLCHAIN }}",
        "          # toolchain: ${{ env.RUSTUP_TOOLCHAIN }}",
        1,
    ),
}
for name, mutation in mutations.items():
    try:
        validate(mutation)
    except ValueError:
        continue
    raise SystemExit(f"toolchain mutation survived: {name}")
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
    scheme-relative-external)
      rewrite_repo_link "//crates.io/crates/assay-cli"
      expect_fail "$name" "external link must be absolute"
      ;;
    blob-HEAD)
      rewrite_repo_link "https://github.com/Rul1an/assay/blob/HEAD/"
      expect_fail "$name" "mutable git ref"
      ;;
    blob-main)
      rewrite_repo_link "https://github.com/Rul1an/assay/blob/main/"
      expect_fail "$name" "mutable git ref"
      ;;
    blob-main-fragment)
      rewrite_repo_link "https://github.com/Rul1an/assay/blob/main#readme"
      expect_fail "$name" "mutable git ref"
      ;;
    blob-main-query)
      rewrite_repo_link "https://github.com/Rul1an/assay/blob/main?plain=1"
      expect_fail "$name" "mutable git ref"
      ;;
    blob-feature-branch)
      rewrite_repo_link "https://github.com/Rul1an/assay/blob/feature/README.md"
      expect_fail "$name" "mutable git ref"
      ;;
    blob-other-repo-main)
      rewrite_repo_link "https://github.com/example/project/blob/main/README.md"
      expect_fail "$name" "mutable git ref"
      ;;
    blob-userinfo-main)
      rewrite_repo_link "https://user@github.com/Rul1an/assay/blob/main/README.md"
      expect_fail "$name" "mutable git ref"
      ;;
    blob-port-main)
      rewrite_repo_link "https://github.com:443/Rul1an/assay/blob/main/README.md"
      expect_fail "$name" "mutable git ref"
      ;;
    blob-scheme-relative-main)
      rewrite_repo_link "//github.com/Rul1an/assay/blob/main/README.md"
      expect_fail "$name" "mutable git ref"
      ;;
    blob-www-main)
      rewrite_repo_link "https://www.github.com/Rul1an/assay/blob/main/README.md"
      expect_fail "$name" "mutable git ref"
      ;;
    blob-trailing-dot-main)
      rewrite_repo_link "https://github.com./Rul1an/assay/blob/main/README.md"
      expect_fail "$name" "mutable git ref"
      ;;
    blob-commit)
      rewrite_repo_link "https://github.com/Rul1an/assay/blob/0123456789abcdef0123456789abcdef01234567/README.md"
      expect_pass "$name"
      ;;
    blob-refs-heads-main)
      rewrite_repo_link "https://github.com/Rul1an/assay/blob/refs/heads/main/"
      expect_fail "$name" "mutable git ref"
      ;;
    raw-github-main)
      rewrite_repo_link "https://raw.githubusercontent.com/Rul1an/assay/main/README.md"
      expect_fail "$name" "mutable git ref"
      ;;
    raw-github-commit)
      rewrite_repo_link "https://raw.githubusercontent.com/Rul1an/assay/0123456789abcdef0123456789abcdef01234567/README.md"
      expect_pass "$name"
      ;;
    github-raw-main)
      rewrite_repo_link "https://github.com/Rul1an/assay/raw/main/README.md"
      expect_fail "$name" "mutable git ref"
      ;;
    github-raw-commit)
      rewrite_repo_link "https://github.com/Rul1an/assay/raw/0123456789abcdef0123456789abcdef01234567/README.md"
      expect_pass "$name"
      ;;
    archive-main)
      rewrite_repo_link "https://github.com/Rul1an/assay/archive/refs/heads/main.zip"
      expect_fail "$name" "mutable git ref"
      ;;
    archive-commit)
      rewrite_repo_link "https://github.com/Rul1an/assay/archive/0123456789abcdef0123456789abcdef01234567.zip"
      expect_pass "$name"
      ;;
    codeload-main)
      rewrite_repo_link "https://codeload.github.com/Rul1an/assay/tar.gz/refs/heads/main"
      expect_fail "$name" "mutable git ref"
      ;;
    codeload-commit)
      rewrite_repo_link "https://codeload.github.com/Rul1an/assay/tar.gz/0123456789abcdef0123456789abcdef01234567"
      expect_pass "$name"
      ;;
    codeload-legacy-main)
      rewrite_repo_link "https://codeload.github.com/Rul1an/assay/legacy.tar.gz/main"
      expect_fail "$name" "mutable git ref"
      ;;
    codeload-legacy-commit)
      rewrite_repo_link "https://codeload.github.com/Rul1an/assay/legacy.tar.gz/0123456789abcdef0123456789abcdef01234567"
      expect_pass "$name"
      ;;
    codeload-unknown-route)
      rewrite_repo_link "https://codeload.github.com/Rul1an/assay/legacy.tar.bz2/main"
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
    version-pinned-shell-prompt)
      rewrite_install '$ cargo install assay-cli@5.4.0 --locked'
      expect_fail "$name" "version pin"
      ;;
    version-pinned-env-prefix)
      rewrite_install "env CARGO_HOME=/tmp cargo install assay-cli@5.4.0 --locked"
      expect_fail "$name" "version pin"
      ;;
    version-pinned-sudo-prefix)
      rewrite_install "sudo cargo install assay-cli@5.4.0 --locked"
      expect_fail "$name" "version pin"
      ;;
    version-pinned-command-prefix)
      rewrite_install "command cargo install assay-cli@5.4.0 --locked"
      expect_fail "$name" "version pin"
      ;;
    version-pinned-path-cargo)
      rewrite_install "/usr/bin/cargo install assay-cli@5.4.0 --locked"
      expect_fail "$name" "version pin"
      ;;
    version-pinned-inline-code)
      rewrite_install "Use \`cargo install assay-cli@5.4.0 --locked\`."
      expect_fail "$name" "version pin"
      ;;
    version-unpinned-followed-by-cargo-version)
      rewrite_install "cargo install assay-cli --locked && cargo --version"
      expect_pass "$name"
      ;;
    install-command-missing)
      rewrite_install "Install assay-cli from crates.io."
      expect_fail "$name" "required unpinned install command"
      ;;
    install-command-wrong-package)
      rewrite_install "cargo install assay --locked"
      expect_fail "$name" "required unpinned install command"
      ;;
    install-command-unlocked)
      rewrite_install "cargo install assay-cli"
      expect_fail "$name" "required unpinned install command"
      ;;
    package-grew-docs)
      run_forbidden_package_member
      echo "PASS: $name"
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
    oversized-gnu-longname)
      run_oversized_gnu_longname
      echo "PASS: $name"
      ;;
    packaged-manifest-source)
      run_packaged_manifest_source
      echo "PASS: $name"
      ;;
    missing-consumer-manifest)
      run_consumer_manifest_failure "$name" "missing Cargo.toml"
      echo "PASS: $name"
      ;;
    disagreeing-consumer-manifest)
      run_consumer_manifest_failure "$name" "not crate-owned README"
      echo "PASS: $name"
      ;;
    unpublished-workspace-dep)
      run_unpublished_workspace_dep
      echo "PASS: $name"
      ;;
    unpublished-workspace-dep-requires-exclude-lockfile)
      run_unpublished_workspace_dep_requires_exclude_lockfile
      ;;
    toolchain-single-source)
      run_toolchain_single_source
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

run_consumer_replay
