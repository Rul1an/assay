#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
READER="${ROOT}/scripts/ci/read-assay-release-tag.sh"
MANIFEST="${ASSAY_WORKSPACE_MANIFEST:-${ROOT}/Cargo.toml}"
MODE="${1:-}"

if [[ -n "${MODE}" && "${MODE}" != "--published" ]]; then
  echo "usage: $0 [--published]" >&2
  exit 2
fi

pin="$(${READER})"
workspace_version="$(
  python3 - "${ROOT}/scripts/ci/lib" "${MANIFEST}" <<'PY'
import sys
from pathlib import Path

sys.path.insert(0, sys.argv[1])
from workspace_version import read_workspace_version

print(read_workspace_version(Path(sys.argv[2])))
PY
)"

python3 - "${pin}" "${workspace_version}" <<'PY'
import re
import sys

pin, workspace = sys.argv[1:]
release = re.fullmatch(
    r"v?([0-9]+\.[0-9]+\.[0-9]+)(-(?:rc|beta)\.(?:0|[1-9][0-9]*))?",
    workspace,
)
if release is None:
    raise SystemExit(f"workspace version is not a supported release version: {workspace}")

def version(value: str) -> tuple[int, int, int]:
    return tuple(map(int, value.removeprefix("v").split(".")))

# A stable install pin at the same core version leads an RC/beta workspace.
pin_version = version(pin)
workspace_core = version(release.group(1))
if pin_version > workspace_core or (
    pin_version == workspace_core and release.group(2) is not None
):
    raise SystemExit(
        f"install pin {pin} leads workspace version {workspace}; "
        "publish the release before advancing the install pin"
    )
PY

if [[ "${MODE}" != "--published" ]]; then
  printf 'assay release pin: %s (workspace %s)\n' "${pin}" "${workspace_version}"
  exit 0
fi

metadata_path=""
metadata_is_temporary=false
cleanup() {
  if [[ "${metadata_is_temporary}" == "true" ]]; then
    rm -f "${metadata_path}"
  fi
}
trap cleanup EXIT

if [[ -n "${ASSAY_RELEASE_METADATA_FILE:-}" ]]; then
  if [[ ! -f "${ASSAY_RELEASE_METADATA_FILE}" ]]; then
    echo "failed to obtain latest published release metadata: ${ASSAY_RELEASE_METADATA_FILE} is missing" >&2
    exit 1
  fi
  metadata_path="${ASSAY_RELEASE_METADATA_FILE}"
else
  # Release existence is an upstream property, including when this runs on a fork.
  repo="${ASSAY_RELEASE_REPOSITORY:-Rul1an/assay}"
  gh_bin="${ASSAY_GH_BIN:-gh}"
  metadata_path="$(mktemp)"
  metadata_is_temporary=true
  if ! "${gh_bin}" api "repos/${repo}/releases/latest" >"${metadata_path}"; then
    echo "failed to obtain latest published release metadata for ${repo}" >&2
    exit 1
  fi
fi

metadata_size="$(wc -c <"${metadata_path}" | tr -d '[:space:]')"
if [[ ! "${metadata_size}" =~ ^[0-9]+$ ]] || ((metadata_size > 1048576)); then
  echo "latest published release metadata exceeds 1048576-byte limit" >&2
  exit 1
fi

python3 - "${pin}" "${metadata_path}" <<'PY'
import json
import re
import sys
from pathlib import Path

pin = sys.argv[1]
try:
    release = json.loads(Path(sys.argv[2]).read_text(encoding="utf-8"))
except (OSError, UnicodeError, json.JSONDecodeError, TypeError) as error:
    raise SystemExit(f"failed to obtain latest published release metadata: {error}")

latest = release.get("tag_name")
if not isinstance(latest, str) or not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+", latest):
    raise SystemExit(f"latest published release has an invalid stable tag: {latest!r}")
if release.get("draft") is not False or release.get("prerelease") is not False:
    raise SystemExit(f"latest published release {latest} is draft or prerelease")

def version(value: str) -> tuple[int, int, int]:
    return tuple(map(int, value.removeprefix("v").split(".")))

if pin != latest:
    if version(pin) > version(latest):
        relation = "leads"
    elif version(pin) < version(latest):
        relation = "trails"
    else:
        raise SystemExit(
            f"install pin {pin} does not exactly match latest published release {latest}"
        )
    raise SystemExit(f"install pin {pin} {relation} latest published release {latest}")

expected_archive = f"assay-{latest}-x86_64-unknown-linux-gnu.tar.gz"
assets = release.get("assets")
asset_names = {
    asset.get("name") for asset in assets if isinstance(asset, dict)
} if isinstance(assets, list) else set()
for expected_asset in (expected_archive, f"{expected_archive}.sha256"):
    if expected_asset not in asset_names:
        raise SystemExit(f"latest published release {latest} lacks {expected_asset}")
PY

# The Homebrew tap is a second install channel for the same release, so a formula that lags the
# published release fails here exactly as a lagging install pin does. The pin equals the latest
# published tag from this point on. Bumping Formula/assay.rb is a release runbook step
# (docs/reference/release.md).
tap_formula_url="${ASSAY_TAP_FORMULA_URL:-https://raw.githubusercontent.com/Rul1an/homebrew-tap/main/Formula/assay.rb}"
tap_targets=(
  aarch64-apple-darwin
  x86_64-apple-darwin
  aarch64-unknown-linux-gnu
  x86_64-unknown-linux-gnu
)
tap_scratch="$(mktemp -d)"
trap 'cleanup; rm -rf "${tap_scratch}"' EXIT

fetch_bounded() {
  local url="$1" out="$2" limit="$3"
  curl -fsSL --proto '=https' --max-filesize "${limit}" -o "${out}" "${url}"
}

if [[ -n "${ASSAY_TAP_FORMULA_FILE:-}" ]]; then
  if [[ ! -f "${ASSAY_TAP_FORMULA_FILE}" ]]; then
    echo "failed to obtain homebrew formula: ${ASSAY_TAP_FORMULA_FILE} is missing" >&2
    exit 1
  fi
  formula_path="${ASSAY_TAP_FORMULA_FILE}"
else
  formula_path="${tap_scratch}/assay.rb"
  if ! fetch_bounded "${tap_formula_url}" "${formula_path}" 65536; then
    echo "failed to obtain homebrew formula from ${tap_formula_url}" >&2
    exit 1
  fi
fi

if [[ -n "${ASSAY_RELEASE_SHA256_DIR:-}" ]]; then
  sidecar_dir="${ASSAY_RELEASE_SHA256_DIR}"
else
  sidecar_dir="${tap_scratch}/sidecars"
  mkdir -p "${sidecar_dir}"
  for target in "${tap_targets[@]}"; do
    sidecar="assay-${pin}-${target}.tar.gz.sha256"
    if ! fetch_bounded \
      "https://github.com/${ASSAY_RELEASE_REPOSITORY:-Rul1an/assay}/releases/download/${pin}/${sidecar}" \
      "${sidecar_dir}/${sidecar}" 4096; then
      echo "failed to obtain release checksum sidecar ${sidecar}" >&2
      exit 1
    fi
  done
fi

python3 - "${pin}" "${formula_path}" "${sidecar_dir}" "${tap_targets[@]}" <<'PY'
import re
import sys
from pathlib import Path

latest, formula_path, sidecar_dir, *targets = sys.argv[1:]
PLATFORM_TARGETS = {
    ("macos", "arm"): "aarch64-apple-darwin",
    ("macos", "intel"): "x86_64-apple-darwin",
    ("linux", "arm"): "aarch64-unknown-linux-gnu",
    ("linux", "intel"): "x86_64-unknown-linux-gnu",
}
assert sorted(PLATFORM_TARGETS.values()) == sorted(targets)
URL = re.compile(
    r"https://github\.com/Rul1an/assay/releases/download/"
    r"(v[0-9]+\.[0-9]+\.[0-9]+)/assay-(v[0-9]+\.[0-9]+\.[0-9]+)-([A-Za-z0-9_.-]+)\.tar\.gz"
)
SHA256 = re.compile(r"[0-9a-f]{64}")


def fail(message):
    raise SystemExit(f"homebrew formula {message}")


try:
    data = Path(formula_path).read_bytes()
    if len(data) > 65536:
        fail("exceeds 65536-byte limit")
    lines = data.decode("utf-8").splitlines()
except (OSError, UnicodeError) as error:
    fail(f"could not be read: {error}")

# Track the enclosing on_macos/on_linux and on_arm/on_intel blocks so a url is bound to the
# platform Homebrew will install it on, not only to the target its file name claims.
stack = []
pairs = {}
pending = None
for number, line in enumerate(lines, 1):
    stripped = line.strip()
    if re.fullmatch(r"on_(macos|linux|arm|intel) do", stripped):
        stack.append(stripped.split()[0].removeprefix("on_"))
    elif stripped.endswith(" do") or re.match(r"(class|def|if|unless|case|begin)\b", stripped):
        stack.append("")
    elif stripped == "end":
        if not stack:
            fail(f"line {number}: unbalanced end")
        stack.pop()
    elif stripped == "url :stable":
        continue
    elif stripped.startswith(("url ", "url(")):
        match = re.fullmatch(r'url "([^"]*)"', stripped)
        if match is None:
            fail(f"line {number}: unsupported url declaration: {stripped}")
        if pending is not None:
            fail(f"line {number}: url for {pending} has no sha256")
        url = URL.fullmatch(match.group(1))
        if url is None:
            fail(f"line {number}: url is not an assay release archive: {match.group(1)}")
        tag, file_tag, target = url.groups()
        if target not in targets:
            fail(f"names unknown target {target}")
        if file_tag != tag:
            fail(f"url for {target} mixes {tag} and {file_tag}")
        if tag != latest:
            fail(f"pins {tag} for {target} but latest published release is {latest}")
        platform = tuple(label for label in stack if label)
        if PLATFORM_TARGETS.get(platform) != target:
            fail(f"places {target} in the {'/'.join(platform) or 'top-level'} block")
        if target in pairs:
            fail(f"declares {target} twice")
        pending = target
    elif stripped.startswith(("sha256 ", "sha256(")):
        match = re.fullmatch(r'sha256 "([^"]*)"', stripped)
        if match is None or SHA256.fullmatch(match.group(1)) is None:
            fail(f"line {number}: unsupported sha256 declaration: {stripped}")
        if pending is None:
            fail(f"line {number}: sha256 without a preceding url")
        pairs[pending] = match.group(1)
        pending = None
    elif stripped.startswith("version "):
        fail(f"line {number}: explicit version declarations are not supported")

if pending is not None:
    fail(f"url for {pending} has no sha256")
if stack:
    fail("has an unclosed block")
for target in targets:
    if target not in pairs:
        fail(f"lacks {target}")

for target in targets:
    archive = f"assay-{latest}-{target}.tar.gz"
    sidecar = Path(sidecar_dir) / f"{archive}.sha256"
    try:
        raw = sidecar.read_bytes()
        if len(raw) > 4096:
            raise SystemExit(f"release checksum sidecar {sidecar.name} exceeds 4096-byte limit")
        text = raw.decode("ascii")
    except (OSError, UnicodeError) as error:
        raise SystemExit(f"failed to obtain release checksum sidecar {sidecar.name}: {error}")
    published = re.fullmatch(r"([0-9a-f]{64})  " + re.escape(archive) + r"\n?", text)
    if published is None:
        raise SystemExit(f"release checksum sidecar {sidecar.name} is malformed")
    if pairs[target] != published.group(1):
        fail(
            f"sha256 for {target} is {pairs[target]} "
            f"but the release sidecar says {published.group(1)}"
        )
PY

printf 'assay published release pin: %s (workspace %s)\n' "${pin}" "${workspace_version}"
printf 'homebrew formula: %s for %s\n' "${pin}" "${tap_targets[*]}"
