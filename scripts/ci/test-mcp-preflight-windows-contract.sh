#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="${ROOT}/.github/workflows/ci.yml"
SELF_TEST=""

case "${1:-}" in
  --self-test)
    SELF_TEST="--self-test"
    WORKFLOW="${2:-$WORKFLOW}"
    ;;
  "")
    ;;
  *)
    WORKFLOW="$1"
    ;;
esac

fail() {
  echo "FAIL: $*" >&2
  return 1
}

check_contract() {
  local workflow="$1"

  python3 - "$workflow" <<'PY'
from pathlib import Path
import re
import sys

workflow = Path(sys.argv[1])
text = workflow.read_text(encoding="utf-8")

job_match = re.search(r"(?ms)^  test:\s*$\n(?P<body>.*?)(?=^  [A-Za-z0-9_-]+:\s*$|\Z)", text)
if job_match is None:
    raise SystemExit("FAIL: ci.yml has no test job")
job = job_match.group(0)

step_name = "Test native Windows MCP preflight resolution"
step_matches = list(
    re.finditer(
        rf"(?ms)^      - name: {re.escape(step_name)}\s*$\n(?P<body>.*?)(?=^      - (?:name:|uses:)|\Z)",
        job,
    )
)
if len(step_matches) != 1:
    raise SystemExit(
        f"FAIL: test job must contain exactly one {step_name!r} step; found {len(step_matches)}"
    )
step = step_matches[0].group(0)

required_lines = (
    "        if: runner.os == 'Windows'",
    "        shell: bash",
)
for line in required_lines:
    if line not in step.splitlines():
        raise SystemExit(f"FAIL: Windows preflight step is missing exact line: {line.strip()}")

run_match = re.search(r"(?ms)^        run: \|\s*$\n(?P<body>(?:^          .*\n?)*)", step)
if run_match is None:
    raise SystemExit("FAIL: Windows preflight step has no block run script")
run = run_match.group("body")
active = "\n".join(
    line.strip() for line in run.splitlines() if line.strip() and not line.lstrip().startswith("#")
)
joined = " ".join(line.removesuffix("\\").strip() for line in active.splitlines())

invocation = (
    "cargo test --locked -p assay-cli --test mcp_preflight_contract -- "
    "native_windows_bare_command_uses_exe_not_pathext_scripts "
    "--exact --test-threads=1 --nocapture 2>&1 | "
    'tee "$RUNNER_TEMP/mcp-preflight-windows.log"'
)
if invocation not in joined:
    raise SystemExit("FAIL: Windows preflight step is missing the exact native test invocation")

success_guard = (
    "grep -F -- "
    "'test native_windows_bare_command_uses_exe_not_pathext_scripts ... ok' "
    '"$RUNNER_TEMP/mcp-preflight-windows.log"'
)
if success_guard not in joined:
    raise SystemExit("FAIL: Windows preflight step is missing the exact test-success guard")

for token in ("GITHUB_SHA", "RUNNER_OS", "ImageOS", "ImageVersion", "rustc -vV"):
    if token not in active:
        raise SystemExit(f"FAIL: Windows preflight step is missing provenance token {token}")

print("PASS: native Windows MCP preflight workflow contract")
PY
}

expect_red() {
  local label="$1"
  local workflow="$2"

  if check_contract "$workflow" >/dev/null 2>&1; then
    fail "mutation stayed green: ${label}"
  fi
  echo "PASS: mutation turns contract red: ${label}"
}

case "$SELF_TEST" in
  "")
    check_contract "$WORKFLOW"
    ;;
  --self-test)
    check_contract "$WORKFLOW"
    scratch="$(mktemp -d "${TMPDIR:-/tmp}/assay-mcp-preflight-windows-contract.XXXXXX")"
    trap 'rm -rf "$scratch"' EXIT

    python3 - "$WORKFLOW" "$scratch" <<'PY'
from pathlib import Path
import sys

source = Path(sys.argv[1]).read_text(encoding="utf-8")
scratch = Path(sys.argv[2])

mutations = {
    "renamed-step.yml": (
        "- name: Test native Windows MCP preflight resolution",
        "- name: Test Windows MCP preflight",
    ),
    "renamed-test.yml": (
        "native_windows_bare_command_uses_exe_not_pathext_scripts \\",
        "native_windows_pathext_smoke \\",
    ),
    "removed-success-guard.yml": (
        "grep -F -- 'test native_windows_bare_command_uses_exe_not_pathext_scripts ... ok' \\",
        "printf '%s\\n' 'native Windows test command completed' \\",
    ),
}

for name, (old, new) in mutations.items():
    if source.count(old) != 1:
        raise SystemExit(f"mutation anchor {old!r} occurs {source.count(old)} times, expected once")
    (scratch / name).write_text(source.replace(old, new, 1), encoding="utf-8")
PY

    expect_red "renamed Windows step" "$scratch/renamed-step.yml"
    expect_red "renamed native test invocation" "$scratch/renamed-test.yml"
    expect_red "removed exact-success guard" "$scratch/removed-success-guard.yml"
    ;;
  *)
    fail "usage: $0 [workflow-path] | $0 --self-test [workflow-path]"
    ;;
esac
