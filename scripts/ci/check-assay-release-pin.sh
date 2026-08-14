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

expected_asset = f"assay-{latest}-x86_64-unknown-linux-gnu.tar.gz"
assets = release.get("assets")
if not isinstance(assets, list) or expected_asset not in {
    asset.get("name") for asset in assets if isinstance(asset, dict)
}:
    raise SystemExit(f"latest published release {latest} lacks {expected_asset}")
PY

printf 'assay published release pin: %s (workspace %s)\n' "${pin}" "${workspace_version}"
