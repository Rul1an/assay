#!/usr/bin/env bash
# shellcheck disable=SC2016 # Assertions intentionally match literal Markdown and Actions syntax.
set -euo pipefail

# shellcheck source=scripts/ci/lib/clear-git-repository-env.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/clear-git-repository-env.sh"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/scripts/ci/lib" "$TMP/docs/generated" "$TMP/docs/guides" "$TMP/.github"
cp "$ROOT/scripts/ci/check-tag-tree-outward-truth.sh" "$TMP/scripts/ci/"
cp "$ROOT/scripts/ci/lib/workspace_version.py" "$TMP/scripts/ci/lib/"
cat > "$TMP/Cargo.toml" <<'EOF'
[workspace.package]
version = "5.3.0"
EOF
cat > "$TMP/docs/generated/agent-golden-path.json" <<'EOF'
{
  "source_version": "5.3.0",
  "source_tag": "v5.3.0",
  "release_version": "5.2.0",
  "release_tag": "v5.2.0"
}
EOF
cat > "$TMP/docs/guides/agent-golden-path.md" <<'EOF'
This source tree declares Assay `5.3.0` (`v5.3.0`).
This journey is pinned to Assay `5.2.0` (`v5.2.0`).
EOF
cat > "$TMP/CHANGELOG.md" <<'EOF'
# Changelog

## [5.3.0] - 2026-08-16
EOF
printf '%s\n' 'v5.2.0' > "$TMP/.github/assay-release-tag"

(
  cd "$TMP"
  git init -q
  git config user.email test@example.invalid
  git config user.name test
  git add -- Cargo.toml CHANGELOG.md .github docs scripts
  git commit -qm fixture
)

head_sha="$(git -C "$TMP" rev-parse HEAD)"
run_check() {
  (cd "$TMP" && CANDIDATE_TAG=v5.3.0 EXPECTED_SHA="$head_sha" \
    bash scripts/ci/check-tag-tree-outward-truth.sh)
}

run_check >/dev/null

mutations=0
mutate_and_fail() {
  local name="$1" file="$2" old="$3" new="$4" diagnostic="$5"
  local backup="$TMP/$file.$name"
  cp "$TMP/$file" "$backup"
  sed "s/$old/$new/g" "$backup" > "$TMP/$file"
  if run_check >"$TMP/$name.out" 2>&1; then
    echo "FAIL: mutation $name was not observed" >&2
    exit 1
  fi
  grep -Fq "$diagnostic" "$TMP/$name.out" || {
    cat "$TMP/$name.out" >&2
    echo "FAIL: mutation $name missed diagnostic: $diagnostic" >&2
    exit 1
  }
  mv "$backup" "$TMP/$file"
  mutations=$((mutations + 1))
  printf 'PASS: %s\n' "$name"
}

mutate_and_fail stale-source-version docs/generated/agent-golden-path.json \
  '"source_version": "5.3.0"' '"source_version": "5.2.0"' 'source_version'
mutate_and_fail stale-source-tag docs/generated/agent-golden-path.json \
  '"source_tag": "v5.3.0"' '"source_tag": "v5.2.0"' 'source_tag'
mutate_and_fail stale-guide-source docs/guides/agent-golden-path.md \
  'source tree declares Assay `5.3.0` (`v5.3.0`)' \
  'source tree declares Assay `5.2.0` (`v5.2.0`)' 'source-tree declaration'
mutate_and_fail stale-changelog CHANGELOG.md \
  '\[5.3.0\]' '[5.2.0]' 'release heading'

if (cd "$TMP" && CANDIDATE_TAG=v5.2.0 EXPECTED_SHA="$head_sha" \
  bash scripts/ci/check-tag-tree-outward-truth.sh) >"$TMP/tag-mismatch.out" 2>&1; then
  echo "FAIL: candidate tag mismatch was not observed" >&2
  exit 1
fi
grep -Fq 'candidate tag v5.2.0 does not match workspace source tag v5.3.0' \
  "$TMP/tag-mismatch.out"
mutations=$((mutations + 1))
printf 'PASS: candidate-tag-mismatch\n'

if (cd "$TMP" && CANDIDATE_TAG=v5.3.0 EXPECTED_SHA=0000000000000000000000000000000000000000 \
  bash scripts/ci/check-tag-tree-outward-truth.sh) >"$TMP/sha-mismatch.out" 2>&1; then
  echo "FAIL: exact SHA mismatch was not observed" >&2
  exit 1
fi
grep -Fq 'checked-out HEAD' "$TMP/sha-mismatch.out"
mutations=$((mutations + 1))
printf 'PASS: exact-sha-mismatch\n'

# Candidate identity and install availability are deliberately separate contracts.
printf '%s\n' 'v4.9.0' > "$TMP/.github/assay-release-tag"
run_check >/dev/null
printf 'PASS: install-pin-does-not-govern-candidate-identity\n'

if [ "$mutations" -ne 6 ]; then
  echo "FAIL: expected 6 observed mutations, got $mutations" >&2
  exit 1
fi
printf 'tag-tree outward-truth mutations: %s observed\n' "$mutations"

check_release_workflow() {
ruby - "$1" <<'RUBY'
require "yaml"

workflow = YAML.safe_load_file(ARGV.fetch(0), aliases: false)
steps = workflow.fetch("jobs", {}).fetch("release-contract", {}).fetch("steps", [])
matching = steps.select { |step| step["name"] == "Verify candidate source-tree identity" }
abort "release workflow must contain one active candidate source-tree guard" unless matching.length == 1
step = matching.fetch(0)
abort "release workflow candidate source-tree guard invokes the wrong command" unless
  step["run"] == "bash scripts/ci/check-tag-tree-outward-truth.sh"
expected_env = {
  "CANDIDATE_TAG" => "${{ steps.version.outputs.version }}",
  "EXPECTED_SHA" => "${{ github.sha }}",
}
abort "release workflow candidate source-tree guard has the wrong environment" unless
  step["env"] == expected_env
abort "release workflow candidate source-tree guard must be unconditional and blocking" if
  step.key?("if") || step.key?("continue-on-error")
RUBY
}

workflow="$ROOT/.github/workflows/release.yml"
check_release_workflow "$workflow"
printf 'PASS: release workflow structurally invokes blocking source-tree guard\n'

python3 - "$workflow" "$TMP/release-disabled.yml" <<'PY'
from pathlib import Path
import sys

text = Path(sys.argv[1]).read_text(encoding="utf-8")
needle = "      - name: Verify candidate source-tree identity\n"
if text.count(needle) != 1:
    raise SystemExit("candidate source-tree guard fixture is ambiguous")
Path(sys.argv[2]).write_text(text.replace(needle, needle + "        if: false\n"), encoding="utf-8")
PY
if check_release_workflow "$TMP/release-disabled.yml" >/dev/null 2>&1; then
  echo "FAIL: disabled release guard was not observed" >&2
  exit 1
fi
printf 'PASS: disabled release guard mutation rejected\n'

python3 - "$workflow" "$TMP/release-commented.yml" <<'PY'
from pathlib import Path
import sys

lines = Path(sys.argv[1]).read_text(encoding="utf-8").splitlines()
start = next(i for i, line in enumerate(lines) if line == "      - name: Verify candidate source-tree identity")
end = next(i for i in range(start + 1, len(lines)) if lines[i].startswith("      - name:"))
for i in range(start, end):
    lines[i] = "# " + lines[i]
Path(sys.argv[2]).write_text("\n".join(lines) + "\n", encoding="utf-8")
PY
if check_release_workflow "$TMP/release-commented.yml" >/dev/null 2>&1; then
  echo "FAIL: commented release guard was not observed" >&2
  exit 1
fi
printf 'PASS: commented release guard mutation rejected\n'

python3 - "$ROOT/.pre-commit-config.yaml" <<'PY'
from pathlib import Path
import re
import sys

lines = Path(sys.argv[1]).read_text(encoding="utf-8").splitlines()
try:
    start = next(i for i, line in enumerate(lines) if line.strip() == "- id: tag-tree-outward-truth")
except StopIteration as exc:
    raise SystemExit("tag-tree outward-truth pre-commit hook is missing") from exc
end = next(
    (i for i in range(start + 1, len(lines)) if lines[i].lstrip().startswith("- id:")),
    len(lines),
)
section = lines[start:end]
selectors = [line.strip().removeprefix("files: ") for line in section if line.strip().startswith("files: ")]
if len(selectors) != 1:
    raise SystemExit("tag-tree outward-truth hook must have exactly one files selector")
pattern = re.compile(selectors[0])
for required in (
    "Cargo.toml",
    "CHANGELOG.md",
    "docs/generated/agent-golden-path.json",
    "docs/guides/agent-golden-path.md",
    "docs/reference/release.md",
    ".github/workflows/release.yml",
    "scripts/ci/check-tag-tree-outward-truth.sh",
    "scripts/ci/test-check-tag-tree-outward-truth.sh",
    "scripts/ci/lib/clear-git-repository-env.sh",
):
    if not pattern.search(required):
        raise SystemExit(f"tag-tree outward-truth hook omits {required}")
PY
printf 'PASS: pre-commit hook covers candidate identity inputs\n'

python3 - "$ROOT/.github/workflows/kernel-matrix.yml" <<'PY'
from pathlib import Path
import sys

lines = Path(sys.argv[1]).read_text(encoding="utf-8").splitlines()
start = next(i for i, line in enumerate(lines) if line.strip() == "paths:")
end = next(i for i in range(start + 1, len(lines)) if lines[i].startswith("  push:"))
paths = {
    line.strip().removeprefix("- ").strip('"')
    for line in lines[start:end]
    if line.strip().startswith("- ")
}
for required in ("Cargo.toml", "CHANGELOG.md", "docs/reference/release.md"):
    if required not in paths:
        raise SystemExit(f"kernel-matrix lint trigger omits candidate identity input: {required}")
PY
printf 'PASS: lint workflow triggers on candidate identity inputs\n'

release_docs="$ROOT/docs/reference/release.md"
required='CANDIDATE_TAG=vX.Y.Z EXPECTED_SHA="$(git rev-parse HEAD)"'
grep -Fq "$required" "$release_docs" || {
  echo "FAIL: release docs omit candidate/install separation: $required" >&2
  exit 1
}
python3 - "$release_docs" <<'PY'
from pathlib import Path
import sys

normalized = " ".join(Path(sys.argv[1]).read_text(encoding="utf-8").split())
required = "The published install pin may still name the previous release"
if required not in normalized:
    raise SystemExit(f"release docs omit candidate/install separation: {required}")
immutable = "Published release tags are immutable and are never moved or rewritten"
if immutable not in normalized:
    raise SystemExit(f"release docs omit tag immutability rule: {immutable}")
PY
printf 'PASS: release docs distinguish candidate identity from installability\n'
