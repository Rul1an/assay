#!/usr/bin/env bash
# Pre-push entry for the Assay Action consumer mutation battery. Required CI runs the battery on
# every change; this only decides whether a push also runs it locally. It skips only when it can
# show that the pushed range changes no battery input, on a checked-out HEAD equal to the pushed
# commit with no tracked change on top. Anything it cannot establish runs the full battery.
set -euo pipefail
[[ "$#" -eq 0 ]] || exit 2
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
BATTERY=scripts/ci/test-check-assay-action-pin.sh
# shellcheck source=scripts/ci/lib/pushed-range-changes.sh
source scripts/ci/lib/pushed-range-changes.sh
changed=""

full() {
  echo "action pin battery: running in full: $1"
  [[ -z "${changed}" ]] || rm -f "${changed}"
  exec bash "${BATTERY}"
}

reason="$(pushed_range_ready)" || full "${reason}"

# The battery copies every listed consumer, so the list comes from the checker that owns it.
listed="$(bash scripts/ci/check-assay-action-pin.sh --list-paths)" || full "consumer list command failed"
[[ -n "${listed}" ]] || full "consumer list is empty"
while IFS= read -r consumer; do
  case "${consumer}" in
    "" | /* | .. | ../* | */.. | */../*) full "consumer list has an invalid entry: '${consumer}'" ;;
  esac
done <<<"${listed}"

# Both sides of a rename and every deleted path, NUL-separated so no name is quoted.
changed="$(mktemp "${TMPDIR:-/tmp}/assay-action-pin-push.XXXXXX")" # BSD mktemp ignores TMPDIR without a template
trap 'rm -f "${changed}"' EXIT # exec skips this trap, so full() removes the file itself
reason="$(pushed_range_paths "${changed}")" || full "${reason}"
count=0
while IFS= read -r -d '' path; do
  count=$((count + 1))
  case "${path}" in
    # The battery reads these directly, or copies the whole directory into its scratch trees.
    scripts/ci/* | .github/* | conformance/* | CHANGELOG.md | docs/PINNED-ACTIONS.md | .pre-commit-config.yaml)
      full "battery input changed: ${path}" ;;
  esac
  while IFS= read -r consumer; do
    [[ "${path}" != "${consumer}" ]] || full "listed consumer changed: ${path}"
  done <<<"${listed}"
done <"${changed}"
echo "action pin battery: skipped; none of the ${count} pushed path(s) is a battery input. Required CI still runs it."
