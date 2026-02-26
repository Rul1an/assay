#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "packs/open/cicd-starter/README.md"
  "docs/ROADMAP.md"
  "scripts/ci/review-adr023-starter-docs-followup.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ADR-023 docs follow-up must not change workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ADR-023 docs follow-up: $f"
    exit 1
  fi
done

echo "[review] README contract checks"
rg -n 'Rul1an/assay/assay-action@[0-9a-f]{40}' packs/open/cicd-starter/README.md >/dev/null || {
  echo "FAIL: README quickstart must pin assay-action to a commit SHA"
  exit 1
}
rg -n 'fail_on:\s*error' packs/open/cicd-starter/README.md >/dev/null || {
  echo "FAIL: README quickstart must show fail_on: error default"
  exit 1
}
rg -n 'fail_on:\s*warning' packs/open/cicd-starter/README.md >/dev/null || {
  echo "FAIL: README must document warning enforcement"
  exit 1
}
rg -n -- '--pack soc2-baseline|soc2-baseline' packs/open/cicd-starter/README.md >/dev/null || {
  echo "FAIL: README next steps must include soc2-baseline"
  exit 1
}
rg -n -- '--pack eu-ai-act-baseline|eu-ai-act-baseline' packs/open/cicd-starter/README.md >/dev/null || {
  echo "FAIL: README next steps must include eu-ai-act-baseline"
  exit 1
}

echo "[review] roadmap marker"
rg -n '^- \[x\] \*\*Docs\*\*: README per ADR-023 Appendix A; pinned GH Action; `--fail-on warning`; Next steps \(follow-up\)$' docs/ROADMAP.md >/dev/null || {
  echo "FAIL: ROADMAP starter docs follow-up marker must be checked"
  exit 1
}

echo "[review] done"
