#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
WRITER="${SCRIPT_DIR}/write_sha256_sidecar.sh"
CI_WORKFLOW="${REPO_ROOT}/.github/workflows/ci.yml"
RELEASE_WORKFLOW="${REPO_ROOT}/.github/workflows/release.yml"
MCPB_BUILDER="${SCRIPT_DIR}/build_mcpb_bundle.sh"

tmp_root="$(mktemp -d)"
trap 'rm -rf "$tmp_root"' EXIT

release_dir="${tmp_root}/release"
mkdir -p "$release_dir"
asset="${release_dir}/assay-v9.9.9-release-proof-kit.tar.gz"
printf 'release asset fixture\n' >"$asset"

bash "$WRITER" "$asset"

if command -v sha256sum >/dev/null 2>&1; then
  digest="$(sha256sum "$asset" | awk '{print $1}')"
else
  digest="$(shasum -a 256 "$asset" | awk '{print $1}')"
fi

printf '%s  %s\n' "$digest" "$(basename "$asset")" >"${tmp_root}/expected.sha256"
cmp "${tmp_root}/expected.sha256" "${asset}.sha256"

(
  cd "$release_dir"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum -c "$(basename "${asset}.sha256")" >/dev/null
  else
    shasum -a 256 -c "$(basename "${asset}.sha256")" >/dev/null
  fi
)

python3 - "$CI_WORKFLOW" "$RELEASE_WORKFLOW" "$MCPB_BUILDER" <<'PY'
import re
import sys
from pathlib import Path

ci_path, release_path, mcpb_path = map(Path, sys.argv[1:])
ci_text = ci_path.read_text()
release_text = release_path.read_text()
mcpb_text = mcpb_path.read_text()

workflow_calls = (
    'bash scripts/ci/write_sha256_sidecar.sh "dist/${ARCHIVE_NAME}.${{ matrix.archive }}"',
    'bash "${GITHUB_WORKSPACE}/scripts/ci/write_sha256_sidecar.sh" "${ARCHIVE_NAME}.tar.gz"',
    'bash scripts/ci/write_sha256_sidecar.sh "release/assay-${VERSION}-sbom-cyclonedx.tar.gz"',
    'bash scripts/ci/write_sha256_sidecar.sh "${OUT_SUMMARY}"',
    'bash scripts/ci/write_sha256_sidecar.sh "${OUT_ARCHIVE}"',
)
mcpb_call = 'bash "${SCRIPT_DIR}/write_sha256_sidecar.sh" "$OUTPUT"'
legacy_writer = re.compile(
    r"(shasum -a 256|sha256sum|Get-FileHash|Out-File).*[.]sha256"
)


def active_lines(text):
    return [
        line.strip()
        for line in text.splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    ]


def validate_wiring(workflow, mcpb):
    workflow_active = active_lines(workflow)
    mcpb_active = active_lines(mcpb)
    for expected in workflow_calls:
        if workflow_active.count(expected) != 1:
            raise ValueError(f"missing or duplicate active release checksum call: {expected}")
    if mcpb_active.count(mcpb_call) != 1:
        raise ValueError("missing or duplicate active MCPB checksum call")
    if any(legacy_writer.search(line) for line in workflow_active + mcpb_active):
        raise ValueError("release checksum producer bypasses write_sha256_sidecar.sh")


validate_wiring(release_text, mcpb_text)

# Pin the original failure in the textual guard: a commented producer is not
# an active producer even though a plain occurrence count still sees it.
provenance_call = workflow_calls[3]
mutated = release_text.replace(
    f"          {provenance_call}",
    f"          # {provenance_call}",
    1,
)
try:
    validate_wiring(mutated, mcpb_text)
except ValueError:
    pass
else:
    raise SystemExit("commented checksum producer passed the wiring contract")

start = ci_text.index("  release-asset-contract:")
remaining = ci_text[start + 1 :]
next_job = re.search(r"^  [a-zA-Z0-9_-]+:$", remaining, re.MULTILINE)
section = ci_text[start : start + 1 + next_job.start()] if next_job else ci_text[start:]
matrix_contract = (
    "name: Release asset contract (${{ matrix.os }})",
    "runs-on: ${{ matrix.os }}",
    "- ubuntu-latest",
    "- windows-latest",
)


def validate_matrix(text):
    active = active_lines(text)
    for required in matrix_contract:
        if active.count(required) != 1:
            raise ValueError(f"release asset contract matrix is missing: {required}")
    if any(line.split(":", 1)[0] == "exclude" for line in active):
        raise ValueError("release asset contract matrix may not exclude an OS")


validate_matrix(section)
matrix_mutations = [
    section.replace("          - windows-latest", "          # - windows-latest", 1),
    section.replace(
        "          - windows-latest",
        "          - windows-latest\n        exclude:\n          - os: windows-latest",
        1,
    ),
]
for mutated in matrix_mutations:
    try:
        validate_matrix(mutated)
    except ValueError:
        continue
    raise SystemExit("release asset contract matrix bypass mutation passed")
PY

echo "sha256 sidecar writer tests passed"
