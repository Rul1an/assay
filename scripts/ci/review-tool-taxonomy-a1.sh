#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/ADR-027-Tool-Taxonomy.md"
  "schemas/tool_taxonomy_v1.schema.json"
  "scripts/ci/review-tool-taxonomy-a1.sh"
)

while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  [[ "$f" == .github/workflows/* ]] && {
    echo "FAIL: tool taxonomy A1 must not touch workflows ($f)"
    exit 1
  }
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  [[ "$ok" != "true" ]] && {
    echo "FAIL: file not allowed in tool taxonomy A1: $f"
    exit 1
  }
done < <(git diff --name-only "$BASE_REF"...HEAD)

python3 -m json.tool schemas/tool_taxonomy_v1.schema.json >/dev/null || {
  echo "FAIL: tool taxonomy schema is not valid JSON"
  exit 1
}

rg -n 'class-based route policy|class-based route matching|tool taxonomy' docs/architecture/ADR-027-Tool-Taxonomy.md >/dev/null || {
  echo "FAIL: ADR missing tool taxonomy marker"
  exit 1
}
rg -n 'matched_tool_classes|matched_route_rule_id|reason_code' docs/architecture/ADR-027-Tool-Taxonomy.md >/dev/null || {
  echo "FAIL: ADR missing decision/evidence reporting requirements"
  exit 1
}
rg -n 'tool-taxonomy-v1' schemas/tool_taxonomy_v1.schema.json >/dev/null || {
  echo "FAIL: schema missing tool-taxonomy-v1 version marker"
  exit 1
}
rg -n '"tool_classes"' schemas/tool_taxonomy_v1.schema.json >/dev/null || {
  echo "FAIL: schema missing tool_classes map"
  exit 1
}

echo "[review] done"
