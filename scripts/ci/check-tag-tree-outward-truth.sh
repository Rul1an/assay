#!/usr/bin/env bash
# Bind a release request to the exact source tree without coupling it to downloadable assets.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CANDIDATE_TAG="${CANDIDATE_TAG:-}"
EXPECTED_SHA="${EXPECTED_SHA:-}"

fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

for required_file in Cargo.toml CHANGELOG.md docs/generated/agent-golden-path.json docs/guides/agent-golden-path.md; do
  [ -f "$ROOT/$required_file" ] || fail "required candidate source file is missing: $required_file"
done

[[ "$CANDIDATE_TAG" =~ ^v[0-9]+[.][0-9]+[.][0-9]+(-(rc|beta)[.][0-9]+)?$ ]] ||
  fail "candidate tag must be vX.Y.Z, vX.Y.Z-rc.N, or vX.Y.Z-beta.N"
[[ "$EXPECTED_SHA" =~ ^[0-9a-f]{40}$ ]] || fail "expected SHA must be 40 lowercase hex characters"

actual_sha="$(git -C "$ROOT" rev-parse HEAD)"
[ "$actual_sha" = "$EXPECTED_SHA" ] ||
  fail "checked-out HEAD $actual_sha does not match requested release SHA $EXPECTED_SHA"

source_version="$({
  PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$ROOT/scripts/ci/lib" \
    python3 - "$ROOT/Cargo.toml" <<'PY'
from pathlib import Path
import sys
from workspace_version import read_workspace_version

print(read_workspace_version(Path(sys.argv[1])))
PY
})"
source_tag="v${source_version}"
[ "$CANDIDATE_TAG" = "$source_tag" ] ||
  fail "candidate tag $CANDIDATE_TAG does not match workspace source tag $source_tag"

IFS=$'\t' read -r contract_source_version contract_source_tag < <(
  python3 - "$ROOT/docs/generated/agent-golden-path.json" <<'PY'
import json
from pathlib import Path
import sys

path = Path(sys.argv[1])
try:
    value = json.loads(path.read_text(encoding="utf-8"))
except Exception as exc:
    raise SystemExit(f"golden-path contract is not readable JSON: {exc}")
fields = []
for key in ("source_version", "source_tag"):
    field = value.get(key)
    if not isinstance(field, str) or not field:
        raise SystemExit(f"golden-path {key} must be a non-empty string")
    fields.append(field)
print("\t".join(fields))
PY
)

[ "$contract_source_version" = "$source_version" ] ||
  fail "golden-path source_version $contract_source_version does not match $source_version"
[ "$contract_source_tag" = "$source_tag" ] ||
  fail "golden-path source_tag $contract_source_tag does not match $source_tag"

guide_declaration="This source tree declares Assay \`$source_version\` (\`$source_tag\`)."
[ "$(grep -Fxc "$guide_declaration" "$ROOT/docs/guides/agent-golden-path.md" || true)" -eq 1 ] ||
  fail "golden-path guide source-tree declaration does not match $source_tag"

changelog_version="${source_version//./[.]}"
changelog_pattern="^## \\[${changelog_version}\\] - [0-9]{4}-[0-9]{2}-[0-9]{2}$"
[ "$(grep -Ec "$changelog_pattern" "$ROOT/CHANGELOG.md" || true)" -eq 1 ] ||
  fail "CHANGELOG.md must contain exactly one dated release heading for $source_version"

printf 'tag-tree outward truth: source %s, candidate %s, checkout %s\n' \
  "$source_version" "$CANDIDATE_TAG" "$actual_sha"
