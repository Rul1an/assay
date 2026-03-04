#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/ADR-028-Coverage-Report.md"
  "schemas/coverage_report_v1.schema.json"
  "scripts/ci/review-coverage-a3.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: coverage A3 must not touch workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in coverage A3: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] marker checks"
rg -n '^# ADR-028: Coverage Report' docs/architecture/ADR-028-Coverage-Report.md >/dev/null || {
  echo "FAIL: ADR title missing"
  exit 1
}
rg -n 'tools_seen' docs/architecture/ADR-028-Coverage-Report.md >/dev/null || {
  echo "FAIL: ADR missing tools_seen definition"
  exit 1
}
python3 - <<'PY'
import json
from pathlib import Path
p = Path("schemas/coverage_report_v1.schema.json")
obj = json.loads(p.read_text(encoding="utf-8"))
assert obj["properties"]["schema_version"]["const"] == "coverage_report_v1"
assert "tools" in obj["properties"]
assert "taxonomy" in obj["properties"]
assert "routes" in obj["properties"]
assert "findings" in obj["properties"]
print("ok")
PY

echo "[review] done"
