#!/usr/bin/env bash
#
# Every place that must name the current version names it. #1996.
#
# A release prep touches a fixed set of files and nothing checked that it did: three of them sat a
# release behind from v3.37.0 onward, and the internal dependency declarations had drifted as far
# back as 3.19.1 while the workspace was at 3.38.0.
#
# WHY THIS IS NOT A GREP FOR OLD VERSION STRINGS
#
# Most version strings in this repo are supposed to be stale, because they record what happened
# rather than what is current:
#
#   - CHANGELOG.md entries.
#   - "Releases starting with 3.36.0 declare Rust 1.89 as their MSRV" in
#     docs/getting-started/installation.md and docs/reference/rust-support.md. That sentence is
#     permanently true and bumping the number would make it false.
#   - Dated status syncs in docs/ROADMAP.md, which narrate a specific release line.
#   - Recorded measurement provenance, e.g. docs/interop/sep2828-decision-pairing-v0.md.
#
# A repo-wide "no old versions" check flags exactly those, gets suppressed, and then catches
# nothing. So this script checks only facts it can DERIVE, and never scans for version-shaped text:
#
#   1. Internal dependency declarations. Enumerated from the root Cargo.toml itself, not from a
#      list kept here, so a new workspace crate is covered the day it is added.
#   2. The `assay --version` sample output in the installation guide, compared against what the
#      binary actually prints, so the expectation comes from the program rather than from a
#      hard-coded string.
#   3. Every tracked `Cargo.lock` that pins a workspace crate. The root lock is the obvious one and
#      `fuzz/Cargo.lock` is the one that was missed: it is a separate workspace, so a release bump
#      that touched only the root lock left it pinning the previous version, and the fuzz job failed
#      on `--locked` after the release PR was already open. The set is discovered with `git ls-files`
#      rather than listed, so a third lockfile is covered the day it appears.
#
# Nothing is excluded by pattern. Lines that must stay historical are out of scope because they are
# not derived from the current version in the first place.
#
# Usage: scripts/ci/check-release-surface.sh
#        ASSAY_BIN=path/to/assay scripts/ci/check-release-surface.sh   (skips cargo build)

set -euo pipefail

cd "$(dirname "$0")/../.."

failures=0
fail() {
  failures=$((failures + 1))
  printf 'FAIL: %s\n' "$*"
}
note() { printf '%s\n' "$*"; }

# ---------------------------------------------------------------------------
# The one source of truth.
# ---------------------------------------------------------------------------
WORKSPACE_VERSION="$(
  awk '
    /^\[workspace\.package\]/ { in_section = 1; next }
    /^\[/                     { in_section = 0 }
    in_section && /^version *=/ {
      gsub(/^version *= *"|".*$/, "")
      print
      exit
    }
  ' Cargo.toml
)"

if [ -z "$WORKSPACE_VERSION" ]; then
  echo "could not read [workspace.package] version from Cargo.toml" >&2
  exit 2
fi
note "workspace version: $WORKSPACE_VERSION"

# ---------------------------------------------------------------------------
# 1. Internal dependency declarations, enumerated from the manifest.
#
# Every crate in this workspace sets `version.workspace = true`, so each one is published at
# WORKSPACE_VERSION. A declaration naming an older version still resolves, because ^3.19.1 is
# satisfied by 3.38.0, which is why this drifted silently for so long. What it costs is a published
# requirement looser than the truth: a consumer that pins the declared minimum gets a version this
# workspace has not compiled against since.
# ---------------------------------------------------------------------------
note ""
note "internal dependency declarations:"

checked=0
while IFS=$'\t' read -r name declared; do
  checked=$((checked + 1))
  if [ "$declared" != "$WORKSPACE_VERSION" ]; then
    fail "Cargo.toml: $name declares version \"$declared\", workspace is \"$WORKSPACE_VERSION\""
  fi
done < <(
  awk '
    /^\[workspace\.dependencies\]/ { in_section = 1; next }
    /^\[/                          { in_section = 0 }
    in_section && /path *= *"(crates|assay-python-sdk)/ {
      name = $1
      if (match($0, /version *= *"[^"]+"/)) {
        v = substr($0, RSTART, RLENGTH)
        gsub(/version *= *"|"/, "", v)
        printf "%s\t%s\n", name, v
      }
    }
  ' Cargo.toml
)

if [ "$checked" -eq 0 ]; then
  fail "no internal path dependencies found in [workspace.dependencies]; the enumeration is broken"
else
  note "  checked $checked declaration(s)"
fi

# ---------------------------------------------------------------------------
# 2. The documented `assay --version` output.
#
# Derived from the binary rather than compared to a literal, so this cannot drift into asserting a
# version the program does not print.
# ---------------------------------------------------------------------------
note ""
note "workspace lockfiles:"

# A lockfile that pins a workspace crate at the previous version is not cosmetic: any job running
# with `--locked` refuses outright. `fuzz/Cargo.lock` is a separate workspace, so the root bump does
# not reach it, and nothing else in this script would have looked.
#
# Discovered, not listed. Only locks that actually pin one of our crates are considered, so the
# vendored upstream reference lock under scripts/ci/fixtures is out of scope by construction rather
# than by an exclusion someone has to maintain.
# The crates this workspace publishes, read from their own manifests. `assay-fuzz` is deliberately
# excluded by this derivation rather than by name: it lives outside `crates/`, pins itself at 0.0.0
# and is never published, so a name-prefix match would have flagged it forever.
WORKSPACE_MEMBERS="$(
  for manifest in crates/*/Cargo.toml assay-python-sdk/Cargo.toml; do
    [ -f "$manifest" ] || continue
    grep -q '^version\.workspace = true' "$manifest" || continue
    awk -F' *= *' '/^name *=/ { gsub(/"/, "", $2); print $2; exit }' "$manifest"
  done | tr '\n' ' '
)"
[ -n "$WORKSPACE_MEMBERS" ] || fail "could not derive the workspace member set; the manifest shape moved"

locks_checked=0
while IFS= read -r lock; do
  [ -f "$lock" ] || continue
  pinned="$(awk -v members="$WORKSPACE_MEMBERS" '
    BEGIN { split(members, m, " "); for (i in m) is_member[m[i]] = 1 }
    /^\[\[package\]\]/ { name = ""; next }
    /^name = / { n = $3; gsub(/"/, "", n); if (n in is_member) name = n; next }
    name != "" && /^version = / {
      v = $3; gsub(/"/, "", v)
      printf "%s\t%s\n", name, v
      name = ""
    }
  ' "$lock")"
  [ -n "$pinned" ] || continue
  locks_checked=$((locks_checked + 1))
  while IFS=$'\t' read -r name version; do
    [ -n "$name" ] || continue
    if [ "$version" != "$WORKSPACE_VERSION" ]; then
      fail "$lock: $name pinned at \"$version\", workspace is \"$WORKSPACE_VERSION\""
    fi
  done <<< "$pinned"
done < <(git ls-files '*Cargo.lock')

note "  checked ${locks_checked} lockfile(s) pinning workspace crates"

note ""
note "documented CLI version output:"

INSTALL_DOC="docs/getting-started/installation.md"
ASSAY_BIN="${ASSAY_BIN:-}"

if [ -z "$ASSAY_BIN" ]; then
  if [ -x "target/debug/assay" ]; then
    ASSAY_BIN="target/debug/assay"
  elif [ -x "target/release/assay" ]; then
    ASSAY_BIN="target/release/assay"
  fi
fi

if [ -n "$ASSAY_BIN" ]; then
  ACTUAL="$("$ASSAY_BIN" --version | head -1 | tr -d '\r')"
  note "  binary prints: $ACTUAL"
  if ! grep -qxF "$ACTUAL" "$INSTALL_DOC"; then
    fail "$INSTALL_DOC does not show \"$ACTUAL\" as the expected \`assay --version\` output"
  fi
else
  # No binary to ask, so fall back to the manifest. Still derived, one step further removed.
  note "  no assay binary available; comparing against the manifest version instead"
  if ! grep -qxF "assay $WORKSPACE_VERSION" "$INSTALL_DOC"; then
    fail "$INSTALL_DOC does not show \"assay $WORKSPACE_VERSION\" as the expected \`assay --version\` output"
  fi
fi

# ---------------------------------------------------------------------------
# 3. Active outward installation claims.
#
# These checks are deliberately path- and claim-scoped. Historical release notes may name channels
# that were proposed at the time; current entrypoints may advertise only channels with a verified
# release artifact. The Python pattern has a token boundary so the supported `assay-it` package is
# not mistaken for the unrelated `assay` package.
# ---------------------------------------------------------------------------
check_absent_regex() {
  local file="$1" pattern="$2" label="$3"
  if [ ! -f "$file" ]; then
    fail "$file: checked outward document is missing"
    return
  fi
  if grep -Eiq -- "$pattern" "$file"; then
    fail "$file: $label"
  fi
}

for file in \
  docs/getting-started/index.md \
  docs/getting-started/installation.md \
  docs/python-sdk/index.md \
  docs/AIcontext/user-flows.md \
  docs/migration-v1.2.md; do
  check_absent_regex "$file" 'pip(3|x)? install( --user)? assay([^[:alnum:]_-]|$)' \
    'unsupported Python package'
done

check_absent_regex docs/getting-started/installation.md \
  'brew install .*assay' 'unsupported Homebrew channel'
check_absent_regex docs/getting-started/installation.md \
  'scoop (bucket add|install) assay' 'unsupported Scoop channel'
for file in docs/getting-started/installation.md docs/getting-started/ci-integration.md docs/use-cases/air-gapped.md; do
  check_absent_regex "$file" 'ghcr\.io/.*/assay' 'unsupported GHCR image'
done
check_absent_regex docs/getting-started/installation.md \
  'assay-windows-x86_64\.zip' 'obsolete Windows asset name'

if ! grep -qxF "# assay $WORKSPACE_VERSION" docs/reference/cli/index.md; then
  fail "docs/reference/cli/index.md: documented CLI version drift"
fi

# ---------------------------------------------------------------------------
note ""
if [ "$failures" -gt 0 ]; then
  note "release surface: $failures disagreement(s) with [workspace.package] version"
  note ""
  note "If this fired on a release prep, bump the files it names. If it fired on a line that is"
  note "supposed to record history, that line should not be derived from the current version and"
  note "this script should not be looking at it: fix the check, do not suppress it."
  exit 1
fi

note "release surface: consistent at $WORKSPACE_VERSION"
