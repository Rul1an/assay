#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TAG_READER="${ROOT}/scripts/ci/read-assay-release-tag.sh"

# Invoke the Bash-only pin parser from the current POSIX shell context. Native
# Windows Python can select a non-Git Bash executable when starting it itself.
ASSAY_QUICKSTART_PUBLISHED_TAG="$(GITHUB_OUTPUT='' "${TAG_READER}")"
export ASSAY_QUICKSTART_PUBLISHED_TAG

exec python3 "${ROOT}/scripts/ci/test_release_quickstart.py"
