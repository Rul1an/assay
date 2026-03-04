#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/ADR-029-Session-State-Window.md"
  "schemas/session_state_window_v1.schema.json"
  "scripts/ci/review-session-state-a2.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: A2 freeze must not touch workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in A2 freeze: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] marker checks"
rg -n '^# ADR-029: Session & State Window Contract' docs/architecture/ADR-029-Session-State-Window.md >/dev/null || {
  echo "FAIL: ADR title missing"
  exit 1
}
python3 - <<'PY'
import json
from pathlib import Path
p = Path("schemas/session_state_window_v1.schema.json")
obj = json.loads(p.read_text(encoding="utf-8"))
assert obj["properties"]["schema_version"]["const"] == "session_state_window_v1"
assert "session" in obj["properties"]
assert "window" in obj["properties"]
assert "snapshot" in obj["properties"]
assert "privacy" in obj["properties"]
print("ok")
PY

echo "[review] done"
