#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

echo "[review] BASE_REF=${BASE_REF}"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/ADR-025-I2-CLOSURE-RELEASE-INTEGRATION.md"
  "schemas/closure_release_policy_v1.json"
  "scripts/ci/review-adr025-i2-step4-a.sh"
)

is_allowlisted() {
  local f="$1"
  for a in "${ALLOWLIST[@]}"; do
    if [[ "$f" == "$a" ]]; then
      return 0
    fi
  done
  return 1
}

echo "[review] diff allowlist + no workflow changes"
changed="$(git diff --name-only "$BASE_REF"...HEAD)"
allowlisted_untracked=""
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if is_allowlisted "$f" || [[ "$f" == .github/workflows/* ]]; then
    if [[ -n "$allowlisted_untracked" ]]; then
      allowlisted_untracked+=$'\n'
    fi
    allowlisted_untracked+="$f"
  fi
done < <(git ls-files --others --exclude-standard)

if [[ -n "$allowlisted_untracked" ]]; then
  if [[ -n "$changed" ]]; then
    changed="$(printf "%s\n%s\n" "$changed" "$allowlisted_untracked")"
  else
    changed="$allowlisted_untracked"
  fi
fi

if [[ -z "$changed" ]]; then
  echo "FAIL: no changes detected vs ${BASE_REF}"
  exit 1
fi

while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: I2 Step4A must not change workflows ($f)"
    exit 1
  fi

  if ! is_allowlisted "$f"; then
    echo "FAIL: file not allowed in I2 Step4A: $f"
    exit 1
  fi
done <<< "$changed"

echo "[review] parse release policy JSON"
python3 - <<'PY'
import json

p = json.load(open('schemas/closure_release_policy_v1.json', 'r', encoding='utf-8'))
required = ['policy_version', 'default_mode', 'supported_modes', 'score_threshold', 'exit_contract']
for key in required:
    if key not in p:
        raise SystemExit(f'Missing policy key: {key}')

if p['default_mode'] not in p['supported_modes']:
    raise SystemExit('default_mode must be in supported_modes')

exit_contract = p['exit_contract']
for key in ('pass', 'policy_fail', 'measurement_fail'):
    if key not in exit_contract:
        raise SystemExit(f'Missing exit_contract key: {key}')

print('release policy json: ok')
PY

echo "[review] done"
