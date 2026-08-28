#!/usr/bin/env bash
#
# Source-version and published-release claims each name their own validated truth. #1996, #2366.
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
#   2. The source binary against the workspace version, and the installation guide against the
#      validated published-release pin. A release-prep branch may legitimately make them differ.
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
# Source-build truth and published-release truth.
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

PUBLISHED_TAG="$(bash scripts/ci/read-assay-release-tag.sh)"
PUBLISHED_VERSION="${PUBLISHED_TAG#v}"
note "published release pin: $PUBLISHED_TAG"

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
# 2. The source binary and documented published `assay --version` output.
#
# A release-prep branch may build the next workspace version while outward installation docs still
# name the latest published release. Verify those facts independently rather than forcing one to
# impersonate the other.
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
  if [ "$ACTUAL" != "assay $WORKSPACE_VERSION" ]; then
    fail "$ASSAY_BIN prints \"$ACTUAL\", workspace is \"assay $WORKSPACE_VERSION\""
  fi
else
  note "  no assay binary available; workspace binary version not driven"
fi
if ! grep -qxF "assay $PUBLISHED_VERSION" "$INSTALL_DOC"; then
  fail "$INSTALL_DOC does not show \"assay $PUBLISHED_VERSION\" as the published \`assay --version\` output"
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

check_contains_fixed() {
  local file="$1" expected="$2" label="$3"
  if [ ! -f "$file" ]; then
    fail "$file: checked outward document is missing"
    return
  fi
  if ! grep -Fq -- "$expected" "$file"; then
    fail "$file: $label"
  fi
}

check_current_release_link() {
  local file="$1" expected="$2"
  local claim_count expected_count
  if [ ! -f "$file" ]; then
    fail "$file: checked outward document is missing"
    return
  fi
  claim_count="$(grep -Fc 'Current release:' "$file" || true)"
  expected_count="$(grep -Fc -- "$expected" "$file" || true)"
  if [ "$claim_count" -ne 1 ] || [ "$expected_count" -ne 1 ]; then
    fail "$file: current release link drift"
  fi
}

check_contains_line() {
  local file="$1" expected="$2" label="$3"
  if [ ! -f "$file" ]; then
    fail "$file: checked outward document is missing"
    return
  fi
  if ! grep -qxF -- "$expected" "$file"; then
    fail "$file: $label"
  fi
}

check_action_refs_pinned() {
  local file="$1"
  local refs invalid
  refs="$(grep -E 'uses:[[:space:]]+' "$file" || true)"
  [ -n "$refs" ] || return
  # Third-party actions stay SHA-pinned. Rul1an/assay-action@v3 is the documented
  # floating line; the executable consumer pin lives in .github/assay-action-pin.
  invalid="$(printf '%s\n' "$refs" | grep -Ev \
    'uses:[[:space:]]+(Rul1an/assay-action@v3|[^[:space:]]+@[0-9a-f]{40})([[:space:]]+#.*)?$' || true)"
  if [ -n "$invalid" ]; then
    fail "$file: GitHub Action references must use a full commit SHA (or Rul1an/assay-action@v3)"
  fi
}

check_install_command_count() {
  local file="$1" expected_count="$2"
  local expected="cargo install assay-cli --version $PUBLISHED_VERSION --locked"
  local all_count current_count
  all_count="$(grep -Ec 'cargo install assay-cli --version [^[:space:]]+ --locked' "$file" || true)"
  current_count="$(grep -Fc "$expected" "$file" || true)"
  if [ "$all_count" -ne "$expected_count" ] || [ "$current_count" -ne "$expected_count" ]; then
    fail "$file: expected $expected_count current release-pinned install command(s)"
  fi
}

check_rust_cli_installs() {
  local file="$1" expected_count="$2"
  check_install_command_count "$file" "$expected_count"
  check_absent_regex "$file" 'cargo install assay([^[:alnum:]_-]|$)' \
    'unsupported Rust CLI package; use assay-cli'
}

is_digest_scoped_rge_bench_claim() {
  local claim="$1"
  [[ "$claim" == *"digest-scoped and does not carry forward"* ]] || return 1
  [[ "$claim" != *"externally reproduced"* ]] || return 1
  printf '%s\n' "$claim" | grep -Eq \
    'v1 71-vector digest `sha256:[0-9a-f]{64}`' || return 1
  printf '%s\n' "$claim" | grep -Eq \
    'current v2 digest `sha256:[0-9a-f]{64}` \(95 vectors\).*one reported \*\*independent implementation\*\*.*v2 95/95 reproduction on 2026-08-24' || return 1
}

check_rge_bench_claims() {
  local link='[RGE-Bench](https://github.com/rge-bench/rge-bench)'
  local claim_prefix="- $link" readme_prefix="- $link — " llms_prefix="- $link: "
  local readme_count llms_count readme_claim llms_claim file claim

  readme_count="$(awk -v prefix="$claim_prefix" 'index($0, prefix) == 1 { count++ } END { print count + 0 }' README.md)"
  llms_count="$(awk -v prefix="$claim_prefix" 'index($0, prefix) == 1 { count++ } END { print count + 0 }' llms.txt)"
  if [ "$readme_count" -ne 1 ]; then
    fail "README.md: expected exactly one RGE-Bench claim"
  fi
  if [ "$llms_count" -ne 1 ]; then
    fail "llms.txt: expected exactly one RGE-Bench claim"
  fi
  if [ "$readme_count" -ne 1 ] || [ "$llms_count" -ne 1 ]; then
    return
  fi

  readme_claim="$(awk -v prefix="$readme_prefix" 'index($0, prefix) == 1 { print substr($0, length(prefix) + 1) }' README.md)"
  llms_claim="$(awk -v prefix="$llms_prefix" 'index($0, prefix) == 1 { print substr($0, length(prefix) + 1) }' llms.txt)"
  if [ "$llms_claim" != "$readme_claim" ]; then
    fail "llms.txt: RGE-Bench claim must match README.md digest scope"
  fi

  for file in README.md llms.txt; do
    if [ "$file" = "README.md" ]; then
      claim="$readme_claim"
    else
      claim="$llms_claim"
    fi
    if ! is_digest_scoped_rge_bench_claim "$claim"; then
      fail "$file: RGE-Bench claim must remain digest-scoped and name current-digest reproduction"
    fi
  done
}

check_rust_cli_installs README.md 1
check_absent_regex README.md 'verified release installer' \
  'installer wording must not imply checksum or provenance verification'
check_absent_regex examples/mcp-quickstart/README.md 'verified release installer' \
  'quickstart installer wording must not imply checksum or provenance verification'
check_absent_regex examples/mcp-quickstart/README.md \
  'root of an extracted CLI release archive|archive carries this bounded quickstart' \
  'quickstart must not claim assets exist in published CLI archives'
check_rust_cli_installs docs/getting-started/index.md 1
check_rust_cli_installs docs/getting-started/installation.md 2
check_rust_cli_installs docs/getting-started/quickstart.md 1
check_rust_cli_installs docs/getting-started/ci-integration.md 4
check_rust_cli_installs docs/reference/cli/index.md 1
check_rust_cli_installs docs/AIcontext/user-flows.md 1
check_rust_cli_installs docs/use-cases/ci-gate.md 1

release_link="Current release: [\`$PUBLISHED_TAG\`](https://github.com/Rul1an/assay/releases/tag/$PUBLISHED_TAG)"
check_current_release_link README.md "$release_link"
check_current_release_link docs/index.md "$release_link"
check_rge_bench_claims

check_absent_regex SECURITY.md '@assay\.dev' \
  'third-party reporting address'
check_absent_regex docs/COMMUNITY.md '@assay\.dev' \
  'third-party reporting address'
check_contains_line SECURITY.md \
  "Assay supports the current published release, **$PUBLISHED_TAG**." \
  "supported release must match $PUBLISHED_TAG"

for file in \
  docs/getting-started/index.md \
  docs/getting-started/installation.md \
  docs/python-sdk/index.md \
  docs/AIcontext/user-flows.md \
  docs/migration-v1.2.md; do
  check_absent_regex "$file" "pip(3|x)? install([[:space:]]+(-U|--upgrade|--user))*[[:space:]]+[\"']?assay([^[:alnum:]_-]|$)" \
    'unsupported Python package'
done

check_absent_regex docs/getting-started/installation.md \
  'brew install .*assay' 'unsupported Homebrew channel'
check_absent_regex docs/getting-started/installation.md \
  'scoop (bucket add|install) assay' 'unsupported Scoop channel'
for file in docs/getting-started/installation.md docs/getting-started/ci-integration.md docs/use-cases/air-gapped.md; do
  check_absent_regex "$file" 'ghcr\.io/.*/assay' 'unsupported GHCR image'
done
linux_archive="assay-$PUBLISHED_TAG-x86_64-unknown-linux-gnu.tar.gz"
linux_archive_root="${linux_archive%.tar.gz}"
linux_archive_url="https://github.com/Rul1an/assay/releases/download/$PUBLISHED_TAG/$linux_archive"
check_contains_line docs/use-cases/air-gapped.md "curl -fLO $linux_archive_url" \
  'current Linux release archive URL drift'
check_contains_line docs/use-cases/air-gapped.md "curl -fLO $linux_archive_url.sha256" \
  'current Linux release checksum URL drift'
check_contains_fixed docs/use-cases/air-gapped.md "$linux_archive_root/assay" \
  'current Linux release archive extraction drift'
check_absent_regex docs/use-cases/air-gapped.md \
  '(image:|docker (pull|run))[[:space:]]+[^[:space:]]*assay' 'unsupported runtime image'
check_absent_regex docs/getting-started/installation.md \
  'assay-windows-x86_64\.zip' 'obsolete Windows asset name'
for file in \
  docs/getting-started/ci-integration.md \
  docs/getting-started/quickstart.md \
  docs/AIcontext/user-flows.md \
  docs/use-cases/ci-gate.md; do
  check_action_refs_pinned "$file"
done

if ! grep -qxF "# assay $PUBLISHED_VERSION" docs/reference/cli/index.md; then
  fail "docs/reference/cli/index.md: documented CLI version drift"
fi
for file in README.md docs/guides/editor-mcp-recipe.md; do
  check_absent_regex "$file" 'assay mcp config-path (codex|<editor>)' \
    'config-path does not support Codex'
done

# ---------------------------------------------------------------------------
note ""
if [ "$failures" -gt 0 ]; then
  note "release surface: $failures disagreement(s) with workspace or published-release truth"
  note ""
  note "During release prep, source files may lead while outward install claims stay on the"
  note "published pin. Move that pin only after the release assets exist; do not suppress the"
  note "distinction or rewrite historical records."
  exit 1
fi

note "release surface: workspace $WORKSPACE_VERSION, published $PUBLISHED_TAG"
