#!/usr/bin/env bash
# Standalone preflight and hook-local skip matching for the golden-path
# hardening script.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
HARDENING="$ROOT/scripts/ci/test-agent-golden-path-skill-hardening.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

[[ -f "$HARDENING" ]] || fail "missing $HARDENING"

empty_path="$TMP/empty-path"
mkdir -p "$empty_path"
if PATH="$empty_path" command -v pre-commit >/dev/null 2>&1; then
  fail "restricted PATH still locates pre-commit"
fi

bash_bin="$(command -v bash)"
[[ -n "$bash_bin" ]] || fail "bash is not on PATH"
set +e
PATH="$empty_path" "$bash_bin" "$HARDENING" >"$TMP/absent.out" 2>&1
absent_rc=$?
set -e
if [[ "$absent_rc" -eq 0 ]]; then
  cat "$TMP/absent.out" >&2
  fail "hardening script exited 0 without pre-commit on PATH"
fi
if ! grep -Fq "pre-commit" "$TMP/absent.out"; then
  cat "$TMP/absent.out" >&2
  fail "absent-pre-commit error did not name pre-commit"
fi
if ! grep -Eq 'pip install pre-commit|install pre-commit' "$TMP/absent.out"; then
  cat "$TMP/absent.out" >&2
  fail "absent-pre-commit error did not say how to install pre-commit"
fi
echo "ok    pre-commit absent names the missing tool and how to install it"

if grep -Fq 'grep -Fq "Skipped"' "$HARDENING"; then
  fail "skip check still matches any line containing Skipped"
fi

helper="$TMP/skip-helper.sh"
python3 - "$HARDENING" "$helper" <<'PY' || fail "could not extract precommit_named_hook_skipped"
from pathlib import Path
import sys

text = Path(sys.argv[1]).read_text(encoding="utf-8")
start = text.find("precommit_named_hook_skipped() {")
if start < 0:
    raise SystemExit(
        "FAIL: hardening script has no precommit_named_hook_skipped helper"
    )
end = text.find("\n}\n", start)
if end < 0:
    raise SystemExit("FAIL: precommit_named_hook_skipped is not a closed function")
Path(sys.argv[2]).write_text(text[start : end + 3], encoding="utf-8")
PY

# shellcheck source=/dev/null
source "$helper"

other="$TMP/other-hook-skipped.log"
cat >"$other" <<'EOF'
trim trailing whitespace.................................................Skipped
Generated-docs drift check self-test.....................................Passed
EOF

if precommit_named_hook_skipped "$other" "Generated-docs drift check self-test"; then
  fail "other hook Skipped was treated as docs-generated-drift-self-test skipped"
fi
if precommit_named_hook_skipped "$other" "docs-generated-drift-self-test"; then
  fail "other hook Skipped was treated as the hook id skipped"
fi
echo "ok    other hook Skipped is not this hook skipped"

own="$TMP/own-hook-skipped.log"
cat >"$own" <<'EOF'
trim trailing whitespace.................................................Passed
Generated-docs drift check self-test.................(no files to check)Skipped
EOF

if ! precommit_named_hook_skipped "$own" "Generated-docs drift check self-test"; then
  fail "this hook's (no files to check)Skipped line was not treated as skipped"
fi
echo "ok    this hook's own skip line is treated as skipped"

echo "golden-path hardening preflight tests passed"
