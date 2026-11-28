#!/usr/bin/env bash
# C2 baseline-only CA carrier. Two explicit modes (pattern from refund-c3-mutation-2026-10/run.sh):
#   check    all tests (the CA scenarios use a synthetic helper subject only), ruff; no C2 child under CA.
#   measure  check, then exactly one baseline-only carrier attempt on the pinned C2 reader, then retention of
#            the whole run root into a fresh destination. A failed or unclosed attempt is retained as well;
#            nothing is retried. Exit: the carrier's status (0/1/2; persistence-failed is 1), 3 when retention
#            or its verification fails, and never 0 when the retained result/provenance pair is incomplete.
set -euo pipefail
cd -- "$(dirname -- "$0")"
usage() {
  echo 'usage: run.sh check CA_GIT_REPOSITORY' >&2
  echo '       run.sh measure CA_GIT_REPOSITORY FRESH_RUN_ROOT FRESH_RETENTION_DEST' >&2
  exit 2
}
[[ $# -ge 2 ]] || usage
mode="$1"
ca="$2"
export REFUND_CA_REPO="$ca"
export PYTHONDONTWRITEBYTECODE=1

checks() {
  python3 -B -m unittest discover -s . -p 'test_*.py' -v
  uvx ruff check --no-cache .
}

case "$mode" in
  check)
    [[ $# -eq 2 ]] || usage
    checks
    ;;
  measure)
    [[ $# -eq 4 ]] || usage
    # Both roots must be fresh before anything runs, so a later directory at $3 is only ever the carrier's own.
    for root in "$3" "$4"; do
      if [[ -e "$root" || -L "$root" ]]; then
        echo "not a fresh path: $root" >&2
        exit 2
      fi
    done
    checks
    set +e
    python3 -B ca_baseline.py --ca-repo "$ca" --run-root "$3"
    status=$?
    set -e
    echo "carrier exit status: $status"
    if [[ -d "$3" && ! -L "$3" ]]; then
      # retain.py exits 4 when it retained everything but the result/provenance pair is incomplete.
      set +e
      python3 -B retain.py --run-root "$3" --dest "$4"
      retained=$?
      [[ $retained -eq 0 || $retained -eq 4 ]] && python3 -B retain.py --verify --dest "$4" >/dev/null
      verified=$?
      set -e
      if [[ $retained -ne 0 && $retained -ne 4 ]] || [[ $verified -ne 0 && $verified -ne 4 ]]; then
        exit 3
      fi
      if [[ $retained -eq 4 && $status -eq 0 ]]; then
        status=1  # never report success for an incomplete pair
      fi
    fi
    exit "$status"
    ;;
  *)
    usage
    ;;
esac
