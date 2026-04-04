#!/usr/bin/env bash
set -euo pipefail

if ! command -v node >/dev/null 2>&1; then
  echo "node is required for this probe" >&2
  exit 1
fi

if ! command -v npx >/dev/null 2>&1; then
  echo "npx is required for this probe" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
FIXTURE_DIR="${REPO_ROOT}/tests/fixtures/interop/protect_mcp_receipts"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT
FAILURES=0

report_failure() {
  local message="$1"
  local stdout_file="$2"
  local stderr_file="$3"

  echo "${message}" >&2
  echo "--- stdout ---" >&2
  cat "${stdout_file}" >&2 || true
  echo "--- stderr ---" >&2
  cat "${stderr_file}" >&2 || true
  FAILURES=$((FAILURES + 1))
}

run_case() {
  local file="$1"
  local expected_exit="$2"
  local expected_stream="$3"
  local expected_substring="$4"
  local stdout_file="${TMP_DIR}/${file}.stdout"
  local stderr_file="${TMP_DIR}/${file}.stderr"

  echo "==> ${file}"

  set +e
  npm_config_loglevel=error npx --yes @veritasacta/verify@0.2.2 \
    "${FIXTURE_DIR}/${file}" >"${stdout_file}" 2>"${stderr_file}"
  local status=$?
  set -e

  if [[ "${status}" -ne "${expected_exit}" ]]; then
    report_failure \
      "unexpected exit code for ${file}: got ${status}, expected ${expected_exit}" \
      "${stdout_file}" \
      "${stderr_file}"
    return
  fi

  case "${expected_stream}" in
    stdout)
      if ! grep -Fq "${expected_substring}" "${stdout_file}"; then
        report_failure \
          "expected stdout for ${file} to contain: ${expected_substring}" \
          "${stdout_file}" \
          "${stderr_file}"
      fi
      ;;
    stderr_nonempty)
      if [[ ! -s "${stderr_file}" ]]; then
        report_failure \
          "expected non-empty stderr for ${file}" \
          "${stdout_file}" \
          "${stderr_file}"
      fi
      ;;
    *)
      echo "unknown expected stream mode: ${expected_stream}" >&2
      exit 1
      ;;
  esac
}

run_case "valid_allow.json" 0 stdout "Verified:"
run_case "valid_deny.json" 0 stdout "Verified:"
run_case "tampered.json" 1 stdout "FAILED:"
run_case "malformed.json" 2 stderr_nonempty ""

echo
if [[ "${FAILURES}" -gt 0 ]]; then
  echo "protect-mcp verifier boundary probe detected ${FAILURES} contract drift issue(s)" >&2
  exit 1
fi

echo "protect-mcp verifier boundary probe passed"
