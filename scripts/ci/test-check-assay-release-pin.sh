#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECKER="${ROOT}/scripts/ci/check-assay-release-pin.sh"
scratch="$(mktemp -d)"
trap 'rm -rf "${scratch}"' EXIT

manifest="${scratch}/Cargo.toml"
pin_file="${scratch}/assay-release-tag"
metadata="${scratch}/release.json"
fake_gh="${scratch}/gh"
formula="${scratch}/assay.rb"
sidecars="${scratch}/sidecars"
mkdir -p "${sidecars}"

# The four Homebrew targets, in the order the formula's platform blocks declare them.
TAP_TARGETS=(
  aarch64-apple-darwin
  x86_64-apple-darwin
  aarch64-unknown-linux-gnu
  x86_64-unknown-linux-gnu
)

fake_sha() {
  local digit
  case "$1" in
    aarch64-apple-darwin) digit=a ;;
    x86_64-apple-darwin) digit=b ;;
    aarch64-unknown-linux-gnu) digit=c ;;
    x86_64-unknown-linux-gnu) digit=d ;;
    *) digit=e ;;
  esac
  printf "${digit}%.0s" {1..64}
}

write_manifest() {
  printf '[workspace.package]\nversion = "%s"\n' "$1" >"${manifest}"
}

write_pin() {
  printf '%s\n' "$1" >"${pin_file}"
}

write_release() {
  local tag="$1"
  local asset="${2:-assay-${tag}-x86_64-unknown-linux-gnu.tar.gz}"
  local checksum="${3:-${asset}.sha256}"
  cat >"${metadata}" <<EOF
{"tag_name":"${tag}","draft":false,"prerelease":false,"assets":[{"name":"${asset}"},{"name":"${checksum}"}]}
EOF
  write_sidecars "${tag}"
  write_formula "${tag}"
}

# Release checksum sidecars, in the release workflow's `<sha256>  <asset>` form.
write_sidecars() {
  local tag="$1" target archive
  rm -f "${sidecars}"/*.sha256
  for target in "${TAP_TARGETS[@]}"; do
    archive="assay-${tag}-${target}.tar.gz"
    printf '%s  %s\n' "$(fake_sha "${target}")" "${archive}" >"${sidecars}/${archive}.sha256"
  done
}

formula_block() {
  local tag="$1" target="$2"
  printf '      url "https://github.com/Rul1an/assay/releases/download/%s/assay-%s-%s.tar.gz"\n' \
    "${tag}" "${tag}" "${target}"
  printf '      sha256 "%s"\n' "$(fake_sha "${target}")"
}

# A formula shaped like Formula/assay.rb in Rul1an/homebrew-tap.
write_formula() {
  local tag="$1"
  {
    printf 'class Assay < Formula\n'
    printf '  desc "Policy-as-code gate for MCP agent tool calls with verifiable evidence"\n'
    printf '  homepage "https://github.com/Rul1an/assay"\n'
    printf '  license "MIT"\n\n'
    printf '  livecheck do\n    url :stable\n    strategy :github_latest\n  end\n\n'
    printf '  on_macos do\n    on_arm do\n'
    formula_block "${tag}" aarch64-apple-darwin
    printf '    end\n    on_intel do\n'
    formula_block "${tag}" x86_64-apple-darwin
    printf '    end\n  end\n\n'
    printf '  on_linux do\n    on_arm do\n'
    formula_block "${tag}" aarch64-unknown-linux-gnu
    printf '    end\n    on_intel do\n'
    formula_block "${tag}" x86_64-unknown-linux-gnu
    printf '    end\n  end\n\n'
    printf '  def install\n    bin.install "assay"\n  end\n\n'
    printf '  test do\n    assert_match "assay #{version}", shell_output("#{bin}/assay --version")\n  end\n'
    printf 'end\n'
  } >"${formula}"
}

# Replace the first formula line containing $1 with $2; an empty $2 deletes the line.
edit_formula() {
  python3 - "${formula}" "$1" "$2" <<'PY'
import sys
from pathlib import Path

path, needle, replacement = Path(sys.argv[1]), sys.argv[2], sys.argv[3]
lines = path.read_text().splitlines(keepends=True)
for i, line in enumerate(lines):
    if needle in line:
        lines[i] = replacement + "\n" if replacement else ""
        break
else:
    raise SystemExit(f"fixture edit found no line containing {needle!r}")
path.write_text("".join(lines))
PY
}

run_check() {
  ASSAY_WORKSPACE_MANIFEST="${manifest}" \
    ASSAY_RELEASE_TAG_FILE="${pin_file}" \
    ASSAY_RELEASE_METADATA_FILE="${metadata}" \
    ASSAY_TAP_FORMULA_FILE="${formula}" \
    ASSAY_RELEASE_SHA256_DIR="${sidecars}" \
    "${CHECKER}" "$@"
}

run_api_check() {
  ASSAY_WORKSPACE_MANIFEST="${manifest}" \
    ASSAY_RELEASE_TAG_FILE="${pin_file}" \
    ASSAY_GH_BIN="${fake_gh}" \
    ASSAY_TAP_FORMULA_FILE="${formula}" \
    ASSAY_RELEASE_SHA256_DIR="${sidecars}" \
    "${CHECKER}" --published
}

expect_fail() {
  local expected="$1"
  shift
  if "$@" >"${scratch}/out" 2>"${scratch}/err"; then
    echo "expected failure containing: ${expected}" >&2
    exit 1
  fi
  if ! grep -Fq -- "${expected}" "${scratch}/err"; then
    echo "failure did not contain '${expected}':" >&2
    cat "${scratch}/err" >&2
    exit 1
  fi
}

echo "== offline steady state =="
write_manifest "5.1.0"
write_pin "v5.1.0"
run_check

echo "== offline release preparation may trail workspace =="
write_manifest "5.2.0"
write_pin "v5.1.0"
run_check

echo "== offline pin may not lead workspace =="
write_manifest "5.1.0"
write_pin "v5.2.0"
expect_fail "install pin v5.2.0 leads workspace version 5.1.0" run_check

echo "== published steady state =="
write_manifest "5.1.0"
write_pin "v5.1.0"
write_release "v5.1.0"
run_check --published

echo "== unpublished release pin fails distinctly =="
write_manifest "5.2.0"
write_pin "v5.2.0"
write_release "v5.1.0"
expect_fail "install pin v5.2.0 leads latest published release v5.1.0" run_check --published

echo "== forgotten post-publish pin fails distinctly =="
write_manifest "5.2.0"
write_pin "v5.1.0"
write_release "v5.2.0"
expect_fail "install pin v5.1.0 trails latest published release v5.2.0" run_check --published

echo "== published pin must match the literal release tag =="
write_manifest "5.1.0"
write_pin "v05.1.0"
write_release "v5.1.0"
expect_fail "install pin v05.1.0 does not exactly match latest published release v5.1.0" run_check --published

echo "== draft release metadata fails closed =="
write_pin "v5.1.0"
cat >"${metadata}" <<'EOF'
{"tag_name":"v5.1.0","draft":true,"prerelease":false,"assets":[{"name":"assay-v5.1.0-x86_64-unknown-linux-gnu.tar.gz"}]}
EOF
expect_fail "latest published release v5.1.0 is draft or prerelease" run_check --published

echo "== invalid published tag fails closed =="
write_release "latest" "assay-latest-x86_64-unknown-linux-gnu.tar.gz"
expect_fail "latest published release has an invalid stable tag: 'latest'" run_check --published

echo "== homebrew formula matching the published release passes =="
write_manifest "5.1.0"
write_pin "v5.1.0"
write_release "v5.1.0"
run_check --published

echo "== homebrew formula one release behind fails =="
write_release "v5.1.0"
write_formula "v5.0.0"
expect_fail "homebrew formula pins v5.0.0 for aarch64-apple-darwin but latest published release is v5.1.0" run_check --published

echo "== homebrew formula with one wrong sha256 fails =="
write_release "v5.1.0"
edit_formula "sha256 \"$(fake_sha x86_64-unknown-linux-gnu)\"" "      sha256 \"$(fake_sha wrong)\""
expect_fail "homebrew formula sha256 for x86_64-unknown-linux-gnu is $(fake_sha wrong) but the release sidecar says $(fake_sha x86_64-unknown-linux-gnu)" run_check --published

echo "== homebrew formula missing a target fails =="
write_release "v5.1.0"
edit_formula "assay-v5.1.0-aarch64-unknown-linux-gnu.tar.gz" ""
edit_formula "sha256 \"$(fake_sha aarch64-unknown-linux-gnu)\"" ""
expect_fail "homebrew formula lacks aarch64-unknown-linux-gnu" run_check --published

echo "== homebrew formula with an extra unknown target fails =="
write_release "v5.1.0"
edit_formula "sha256 \"$(fake_sha x86_64-unknown-linux-gnu)\"" "$(
  printf '      sha256 "%s"\n' "$(fake_sha x86_64-unknown-linux-gnu)"
  formula_block v5.1.0 x86_64-unknown-linux-musl
)"
expect_fail "homebrew formula names unknown target x86_64-unknown-linux-musl" run_check --published

echo "== homebrew formula with arm and intel archives swapped fails =="
write_release "v5.1.0"
# Swap the two macOS blocks' contents: each target keeps its correct sha256, but sits in the
# block Homebrew installs on the other architecture.
python3 - "${formula}" "$(fake_sha aarch64-apple-darwin)" "$(fake_sha x86_64-apple-darwin)" <<'PY'
import sys
from pathlib import Path

path, arm_sha, intel_sha = Path(sys.argv[1]), sys.argv[2], sys.argv[3]
text = path.read_text()
for old, new in (("aarch64-apple-darwin", "x86_64-apple-darwin"), (arm_sha, intel_sha)):
    text = text.replace(old, "\0").replace(new, old).replace("\0", new)
path.write_text(text)
PY
expect_fail "homebrew formula places x86_64-apple-darwin in the macos/arm block" run_check --published

echo "== RC and beta workspaces retain a published stable install pin =="
for candidate in 5.2.0-rc.1 5.2.0-beta.2; do
  write_manifest "$candidate"
  write_pin "v5.1.0"
  write_release "v5.1.0"
  run_check
  run_check --published
  write_pin "v5.2.0"
  expect_fail "install pin v5.2.0 leads workspace version $candidate" run_check
done

echo "== malformed or unsupported workspace channels fail closed =="
write_pin "v5.1.0"
for candidate in 5.2.0-rc 5.2.0-rc.01 5.2.0-alpha.1 5.2.0-rc.1.extra; do
  write_manifest "$candidate"
  expect_fail "workspace version is not a supported release version" run_check
done

echo "== missing install asset fails closed =="
write_manifest "5.2.0"
write_pin "v5.2.0"
write_release "v5.2.0" \
  "assay-v5.2.0-x86_64-unknown-linux-gnu.tar.gz.sha256" \
  "unrelated.txt"
expect_fail "latest published release v5.2.0 lacks assay-v5.2.0-x86_64-unknown-linux-gnu.tar.gz" run_check --published

echo "== missing checksum sidecar fails closed =="
write_release "v5.2.0" \
  "assay-v5.2.0-x86_64-unknown-linux-gnu.tar.gz" \
  "unrelated.txt"
expect_fail "latest published release v5.2.0 lacks assay-v5.2.0-x86_64-unknown-linux-gnu.tar.gz.sha256" run_check --published

echo "== unavailable release metadata fails closed =="
rm -f "${metadata}"
expect_fail "failed to obtain latest published release metadata" run_check --published

echo "== failed GitHub API call fails closed =="
cat >"${fake_gh}" <<'EOF'
#!/usr/bin/env bash
exit 71
EOF
chmod +x "${fake_gh}"
expect_fail "failed to obtain latest published release metadata for Rul1an/assay" run_api_check

echo "== fork CI still queries the authoritative upstream release =="
write_manifest "5.1.0"
write_pin "v5.1.0"
write_release "v5.1.0"
cat >"${fake_gh}" <<'EOF'
#!/usr/bin/env bash
if [[ "$*" != "api repos/Rul1an/assay/releases/latest" ]]; then
  exit 72
fi
cat "${ASSAY_RELEASE_METADATA_FIXTURE}"
EOF
chmod +x "${fake_gh}"
GITHUB_REPOSITORY="contributor/assay" \
  ASSAY_RELEASE_METADATA_FIXTURE="${metadata}" \
  run_api_check

echo "== oversized release metadata fails before parsing =="
python3 - "${metadata}" <<'PY'
import sys
from pathlib import Path

Path(sys.argv[1]).write_bytes(b"x" * (1048576 + 1))
PY
expect_fail "latest published release metadata exceeds 1048576-byte limit" run_check --published

echo "assay release pin contract: PASS"
