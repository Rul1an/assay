#!/usr/bin/env bash
# Pre-push entry for the editor/release-surface consumer battery. Required CI runs the battery on
# every change; this only decides whether a push also runs it locally. It skips only when it can
# show that the pushed range changes no battery input, on a checked-out HEAD equal to the pushed
# commit with no tracked change on top. Anything it cannot establish runs the full battery.
#
# Which paths are inputs is the battery's own answer (`--select-inputs`), read from the protected
# hooks in .pre-commit-config.yaml, so the rule has one owner and no copy here.
set -euo pipefail
[[ "$#" -eq 0 ]] || exit 2
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
BATTERY=scripts/ci/test-editor-release-hook-precommit-consumer.sh
# shellcheck source=scripts/ci/lib/pushed-range-changes.sh
source scripts/ci/lib/pushed-range-changes.sh
changed=""
selected=""

# Removes only what was allocated; an empty name is never passed to rm.
cleanup() {
  local file
  for file in "${changed}" "${selected}"; do
    [[ -z "${file}" ]] || rm -f -- "${file}"
  done
}

full() {
  echo "editor/release consumer: running in full: $1"
  cleanup # exec skips the EXIT trap
  exec bash "${BATTERY}"
}

# These change what the battery reads, whatever the push changed.
for name in ASSAY_BIN ASSAY_EDITOR_RECIPE ASSAY_RELEASE_TAG_FILE; do
  if declare -p "${name}" >/dev/null 2>&1; then
    full "${name} is set, which changes what the battery reads"
  fi
done

reason="$(pushed_range_ready)" || full "${reason}"

# Set before the first allocation, so a failed second one still removes the first.
trap cleanup EXIT
# Both sides of a rename and every deleted path, NUL-separated so no name is quoted.
changed="$(mktemp "${TMPDIR:-/tmp}/assay-editor-release-push.XXXXXX")" # BSD mktemp ignores TMPDIR without a template
selected="$(mktemp "${TMPDIR:-/tmp}/assay-editor-release-inputs.XXXXXX")"
reason="$(pushed_range_paths "${changed}")" || full "${reason}"

bash "${BATTERY}" --select-inputs <"${changed}" >"${selected}" ||
  full "the battery could not read its input rule"
if [[ -s "${selected}" ]]; then
  full "battery input changed: $(head -n 1 "${selected}")"
fi
count="$(tr -cd '\000' <"${changed}" | wc -c | tr -d ' ')"
echo "editor/release consumer: skipped; none of the ${count} pushed path(s) is a battery input. Required CI still runs it."
