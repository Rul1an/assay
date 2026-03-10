#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-2026Q1-RESULTS.md"
  "docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-FIDELITY-HTTP-2026Q1-RESULTS.md"
  "docs/contributing/SPLIT-CHECKLIST-exp-mcp-fragmented-ipi-line-closure.md"
  "docs/contributing/SPLIT-REVIEW-PACK-exp-mcp-fragmented-ipi-line-closure.md"
  "scripts/ci/review-exp-mcp-fragmented-ipi-line-closure.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: closure slice must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in closure slice: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] marker checks: final line summary and bounded claim"
rg -n '^## 2026Q1 line closure summary$' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-2026Q1-RESULTS.md >/dev/null || {
  echo "FAIL: missing line closure heading in main results doc"
  exit 1
}
rg -n '^### Final line table$' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-2026Q1-RESULTS.md >/dev/null || {
  echo "FAIL: missing final line table heading"
  exit 1
}
rg -n '^### Bounded core claim$' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-2026Q1-RESULTS.md >/dev/null || {
  echo "FAIL: missing bounded core claim heading"
  exit 1
}
rg -n '^### Explicit limits$' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-2026Q1-RESULTS.md >/dev/null || {
  echo "FAIL: missing explicit limits heading"
  exit 1
}
rg -n 'Sink-fidelity HTTP \(offline localhost\)' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-2026Q1-RESULTS.md >/dev/null || {
  echo "FAIL: final line table missing sink-fidelity HTTP row"
  exit 1
}
rg -n 'Interleaving \(mixed legit\+malicious\)' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-2026Q1-RESULTS.md >/dev/null || {
  echo "FAIL: final line table missing interleaving row"
  exit 1
}

echo "[review] marker checks: DEC-007 closure note"
rg -n '^## DEC-007 closure note$' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-FIDELITY-HTTP-2026Q1-RESULTS.md >/dev/null || {
  echo "FAIL: missing DEC-007 closure note heading"
  exit 1
}
rg -n '^### Proven in this bounded line$' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-FIDELITY-HTTP-2026Q1-RESULTS.md >/dev/null || {
  echo "FAIL: missing proven-boundary heading"
  exit 1
}
rg -n '^### Not proven$' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-FIDELITY-HTTP-2026Q1-RESULTS.md >/dev/null || {
  echo "FAIL: missing not-proven heading"
  exit 1
}

echo "[review] PASS"
