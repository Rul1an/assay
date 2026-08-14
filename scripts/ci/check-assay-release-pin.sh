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
stable = re.compile(r"^v?[0-9]+\.[0-9]+\.[0-9]+$")
if not stable.fullmatch(workspace):
    raise SystemExit(f"workspace version is not stable semver: {workspace}")

def version(value: str) -> tuple[int, int, int]:
    return tuple(map(int, value.removeprefix("v").split(".")))

if version(pin) > version(workspace):
    raise SystemExit(
        f"install pin {pin} leads workspace version {workspace}; "
        "publish the release before advancing the install pin"
    )
PY

if [[ "${MODE}" != "--published" ]]; then
  printf 'assay release pin: %s (workspace %s)\n' "${pin}" "${workspace_version}"
  exit 0
fi

metadata=""
if [[ -n "${ASSAY_RELEASE_METADATA_FILE:-}" ]]; then
  if [[ ! -f "${ASSAY_RELEASE_METADATA_FILE}" ]]; then
    echo "failed to obtain latest published release metadata: ${ASSAY_RELEASE_METADATA_FILE} is missing" >&2
    exit 1
  fi
  metadata="$(cat "${ASSAY_RELEASE_METADATA_FILE}")"
else
  repo="${GITHUB_REPOSITORY:-Rul1an/assay}"
  gh_bin="${ASSAY_GH_BIN:-gh}"
  if ! metadata="$("${gh_bin}" api "repos/${repo}/releases/latest")"; then
    echo "failed to obtain latest published release metadata for ${repo}" >&2
    exit 1
  fi
fi

python3 - "${pin}" "${metadata}" <<'PY'
import json
import re
import sys

pin = sys.argv[1]
try:
    release = json.loads(sys.argv[2])
except (json.JSONDecodeError, TypeError) as error:
    raise SystemExit(f"failed to obtain latest published release metadata: {error}")

latest = release.get("tag_name")
if not isinstance(latest, str) or not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+", latest):
    raise SystemExit(f"latest published release has an invalid stable tag: {latest!r}")
if release.get("draft") is not False or release.get("prerelease") is not False:
    raise SystemExit(f"latest published release {latest} is draft or prerelease")

def version(value: str) -> tuple[int, int, int]:
    return tuple(map(int, value.removeprefix("v").split(".")))

if version(pin) > version(latest):
    raise SystemExit(f"install pin {pin} leads latest published release {latest}")
if version(pin) < version(latest):
    raise SystemExit(f"install pin {pin} trails latest published release {latest}")

expected_asset = f"assay-{latest}-x86_64-unknown-linux-gnu.tar.gz"
assets = release.get("assets")
if not isinstance(assets, list) or expected_asset not in {
    asset.get("name") for asset in assets if isinstance(asset, dict)
}:
    raise SystemExit(f"latest published release {latest} lacks {expected_asset}")
PY

printf 'assay published release pin: %s (workspace %s)\n' "${pin}" "${workspace_version}"
