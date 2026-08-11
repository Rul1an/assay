#!/usr/bin/env bash
# Stubbed Docker-route coverage for scripts/structurizr-export.sh.
#
# Pins that the Docker fallback mounts the workspace and writes Mermaid under
# /workspace/export without GNU-only `realpath --relative-to` (macOS /bin/realpath
# rejects that flag). Compatible with macOS Bash 3.2.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
EXPORT_SCRIPT="${ROOT}/scripts/structurizr-export.sh"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

[[ -f "${EXPORT_SCRIPT}" ]] || fail "missing scripts/structurizr-export.sh"

if grep -qF 'realpath --relative-to' "${EXPORT_SCRIPT}"; then
  fail "structurizr-export.sh still uses GNU-only realpath --relative-to (breaks macOS)"
fi

SCRATCH="$(mktemp -d)"
trap 'rm -rf "${SCRATCH}"' EXIT
STUB_BIN="${SCRATCH}/bin"
mkdir -p "${STUB_BIN}"
LOG="${SCRATCH}/docker.log"
: >"${LOG}"

cat >"${STUB_BIN}/docker" <<'STUB'
#!/usr/bin/env bash
# Record argv for assertions; succeed without talking to a daemon.
{
  printf 'argv:'
  printf ' %s' "$@"
  printf '\n'
} >>"${STRUCTURIZR_DOCKER_LOG}"
exit 0
STUB
chmod +x "${STUB_BIN}/docker"

# Minimal PATH: stub docker only, no Homebrew/native structurizr-cli directories.
STRUCTURIZR_DOCKER_LOG="${LOG}" PATH="${STUB_BIN}:/usr/bin:/bin" \
  bash "${EXPORT_SCRIPT}" >"${SCRATCH}/out" 2>&1 \
  || fail "export script failed under stubbed docker:
$(cat "${SCRATCH}/out")"

[[ -s "${LOG}" ]] || fail "docker was not invoked"

# Every docker run must mount the workspace dir and write to /workspace/export.
while IFS= read -r line; do
  case "${line}" in
    argv:*)
      printf '%s\n' "${line}" | grep -q ' -v ' \
        || fail "docker argv missing -v volume mount: ${line}"
      printf '%s\n' "${line}" | grep -q ':/workspace' \
        || fail "docker argv missing :/workspace mount: ${line}"
      printf '%s\n' "${line}" | grep -q ' -output /workspace/export' \
        || fail "docker argv missing portable -output /workspace/export: ${line}"
      printf '%s\n' "${line}" | grep -q 'realpath' \
        && fail "docker argv unexpectedly mentions realpath: ${line}"
      ;;
  esac
done <"${LOG}"

echo "ok   structurizr-export docker route uses /workspace/export (portable)"
echo "structurizr-export docker contract: PASS"
