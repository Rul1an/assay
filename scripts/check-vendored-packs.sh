#!/usr/bin/env bash
# Check that vendored packs in crates/assay-evidence/packs/ match packs/open/ source.
# Fails CI if drift detected. Add new pack pairs when adding packs to packs/open/.
#
# Note: mandate-baseline has no packs/open/ source (internal/mandate-specific);
# it is vendored only. Do not add it here.
set -euo pipefail

REPO_ROOT="${REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
cd "$REPO_ROOT"

fail=0

check_pair() {
  local src="$1"
  local vend="$2"

  if [[ ! -f "$src" ]]; then
    echo "ERROR: missing source pack: $src"
    fail=1
    return
  fi
  if [[ ! -f "$vend" ]]; then
    echo "ERROR: missing vendored pack: $vend"
    fail=1
    return
  fi

  # Compare ignoring the "Vendored from ..." header line (if present).
  if ! diff -q <(grep -v '^# Vendored from ' "$vend") "$src" >/dev/null 2>&1; then
    echo "ERROR: vendored pack drift detected:"
    echo "  src : $src"
    echo "  vend: $vend"
    echo
    diff -u <(grep -v '^# Vendored from ' "$vend") "$src" || true
    echo
    echo "Fix: copy $src -> $vend (preserve optional '# Vendored from ...' header)."
    fail=1
  fi
}

check_pair "packs/open/cicd-starter/pack.yaml" "crates/assay-evidence/packs/cicd-starter.yaml"
check_pair "packs/open/eu-ai-act-baseline/pack.yaml" "crates/assay-evidence/packs/eu-ai-act-baseline.yaml"
check_pair "packs/open/soc2-baseline/pack.yaml" "crates/assay-evidence/packs/soc2-baseline.yaml"

exit "$fail"
