#!/usr/bin/env bash
# Self-test for check-editor-mcp-recipe-truth.sh.
#
# A guard that only ever passes proves nothing about its own sensitivity, and the drifts this
# one exists to catch are prose, so the failure mode is a rule that matches nothing. Every case
# below mutates a fixture recipe and asserts the guard fails with the specific diagnostic that
# names the rule. The two "must survive" cases assert the guard also fails when the shipped
# claims are deleted, so a future edit cannot satisfy it by gutting the recipe.
#
# Each case drives the guard through ASSAY_EDITOR_RECIPE against a fixture in a temp dir, so no
# case reads or writes the real doc.

set -euo pipefail

# shellcheck source=scripts/ci/lib/clear-git-repository-env.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/clear-git-repository-env.sh"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
GUARD="$ROOT/scripts/ci/check-editor-mcp-recipe-truth.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cases=0
failures=0

# A fixture that passes: shipped stdio boundary, #2358 linked, both shipped claims present.
write_clean() {
  cat > "$TMP/recipe.md" <<'MD'
# Editor MCP Recipe

## The wrap command

Run `assay mcp wrap` to enforce policy at the protocol boundary.
Use `assay-mcp-server proxy-enforce` for the enforcing path.

## Remote servers

Local stdio only. `assay-mcp-server` negotiates `2024-11-05` and `2025-11-25`; a request
declaring MCP revision 2026-07-28 is refused with JSON-RPC error -32022. The negotiated
modern surface is tracked in https://github.com/Rul1an/assay/issues/2358 and is not delivered.
MD
}

run_guard() { ASSAY_EDITOR_RECIPE="$TMP/recipe.md" bash "$GUARD" 2>&1; }

expect_pass() {
  local label="$1"
  cases=$((cases + 1))
  if out="$(run_guard)"; then
    printf 'ok   %s\n' "$label"
  else
    failures=$((failures + 1))
    printf 'FAIL: %s — guard rejected input it should accept\n%s\n' "$label" "$out"
  fi
}

# expect_fail asserts BOTH a non-zero exit and the exact diagnostic, so a case cannot pass
# because some unrelated rule happened to fire.
expect_fail() {
  local label="$1" expected="$2"
  cases=$((cases + 1))
  if out="$(run_guard)"; then
    failures=$((failures + 1))
    printf 'FAIL: %s — guard accepted input it should reject\n' "$label"
  elif printf '%s' "$out" | grep -Fq -- "$expected"; then
    printf 'ok   %s\n' "$label"
  else
    failures=$((failures + 1))
    printf 'FAIL: %s — rejected, but not for the stated reason (wanted %s)\n%s\n' \
      "$label" "$expected" "$out"
  fi
}

echo '== baseline: a truthful recipe passes =='
write_clean
expect_pass 'clean fixture passes'

echo '== drift 1: stale release-candidate copy =='
write_clean
printf 'This section is provisional against the release candidate.\n' >> "$TMP/recipe.md"
expect_fail 'provisional wording rejected' 'stale release-candidate wording'

echo '== drift 2: future-tense specification promise =='
write_clean
printf 'It will be finalised once the spec is final.\n' >> "$TMP/recipe.md"
expect_fail 'future-tense promise rejected' 'future-tense specification promise'

echo '== drift 3a: remote OAuth/OIDC instruction =='
write_clean
printf 'Align the wrapped server to that OAuth 2.1 / OIDC flow with PKCE.\n' >> "$TMP/recipe.md"
expect_fail 'OAuth instruction rejected' 'remote OAuth/OIDC instruction'

echo '== drift 3b: MCP UI / sandboxed-iframe instruction =='
write_clean
printf 'It renders server UIs in a sandboxed iframe.\n' >> "$TMP/recipe.md"
expect_fail 'MCP UI instruction rejected' 'MCP UI / sandboxed-iframe instruction'

echo '== drift 4a: modern revision named without the tracking issue =='
cat > "$TMP/recipe.md" <<'MD'
# Editor MCP Recipe
Run `assay mcp wrap`; use proxy-enforce for the enforcing path.
Remote work targets MCP revision 2026-07-28.
MD
expect_fail 'unlinked modern revision rejected' 'without linking the implementation issue #2358'

echo '== drift 4b: modern revision advertised as shipped =='
write_clean
printf 'Assay now supports MCP revision 2026-07-28 over remote transport.\n' >> "$TMP/recipe.md"
expect_fail 'shipped-claim wording rejected' 'described as shipped or supported'

echo '== the modern revision may be named when tracked (no false positive) =='
write_clean
expect_pass 'linked, non-shipped mention accepted'

echo '== shipped claims must survive: deleting them fails the guard =='
write_clean
grep -v 'assay mcp wrap' "$TMP/recipe.md" > "$TMP/recipe.tmp" && mv "$TMP/recipe.tmp" "$TMP/recipe.md"
expect_fail 'deleted wrap claim rejected' 'local stdio wrap claim retained'

write_clean
grep -v 'proxy-enforce' "$TMP/recipe.md" > "$TMP/recipe.tmp" && mv "$TMP/recipe.tmp" "$TMP/recipe.md"
expect_fail 'deleted proxy-enforce claim rejected' 'proxy-enforce claim retained'

echo '== false negatives: spelled-out and punctuation variants must be caught =='
# The forbidden-pattern rules are prose matches, so a variant spelling is a silent hole rather
# than a visible failure. Each case below is a real way the same claim gets written.
write_clean
printf 'Align the wrapped server to that OpenID Connect flow.\n' >> "$TMP/recipe.md"
expect_fail 'spelled-out OpenID Connect rejected' 'remote OAuth/OIDC instruction'

write_clean
printf 'It renders a server UI for each tool.\n' >> "$TMP/recipe.md"
expect_fail 'singular "server UI" rejected' 'MCP UI / sandboxed-iframe instruction'

write_clean
printf 'Rendered in a sandboxed-iframe host.\n' >> "$TMP/recipe.md"
expect_fail 'hyphenated sandboxed-iframe rejected' 'MCP UI / sandboxed-iframe instruction'

echo '== CI wiring: a recipe-only change must execute this guard =='
# The guard is only worth its rules if a drift actually reaches it. The pre-commit hook covers
# the local path; CI coverage depends on the one workflow that runs `pre-commit run --all-files`
# selecting the recipe, and `scripts/**` does not help because a recipe-only drift touches no
# script. Asserted here rather than in a workflow-wide contract so the rule that judges this
# file also owns the wiring that reaches it.
cases=$((cases + 1))
if RECIPE_PATH='docs/guides/editor-mcp-recipe.md' python3 - "$ROOT" <<'PY'
import os, re, sys

root = sys.argv[1]
recipe = os.environ["RECIPE_PATH"]
problems = []

# 1. Exactly one workflow runs the full pre-commit pass; find it rather than assuming its name.
wf_dir = os.path.join(root, ".github", "workflows")
runners = [
    name
    for name in sorted(os.listdir(wf_dir))
    if "pre-commit run --all-files" in open(os.path.join(wf_dir, name), encoding="utf-8").read()
]
if len(runners) != 1:
    problems.append(
        f"expected exactly one workflow running `pre-commit run --all-files`, found {runners}"
    )

# 2. That workflow's pull_request.paths must select the recipe, or a recipe-only drift never
#    reaches the guard in CI.
for name in runners:
    text = open(os.path.join(wf_dir, name), encoding="utf-8").read()
    block = re.search(r"(?ms)^  pull_request:\n((?:    .+\n|\n)*)", text)
    if not block:
        problems.append(f"{name}: no pull_request trigger block found")
        continue
    listed = re.findall(r'^\s*-\s*"([^"]+)"', block.group(1), re.M)
    if recipe not in listed:
        problems.append(
            f"{name}: pull_request.paths does not select {recipe}; "
            "a recipe-only drift would not execute the guard in CI"
        )

# 3. The pre-commit hook that runs the guard must also select the recipe locally.
cfg = open(os.path.join(root, ".pre-commit-config.yaml"), encoding="utf-8").read()
hook = re.search(
    r"(?ms)^      - id: editor-mcp-recipe-truth\n(.*?)(?=^      - id: |\Z)", cfg
)
if not hook:
    problems.append(".pre-commit-config.yaml: hook editor-mcp-recipe-truth not found")
else:
    files = re.search(r"^\s*files:\s*(\S+)\s*$", hook.group(1), re.M)
    if not files:
        problems.append("editor-mcp-recipe-truth: no files: selector")
    elif not re.search(files.group(1), recipe):
        problems.append(
            f"editor-mcp-recipe-truth: files: selector does not match {recipe}"
        )

if problems:
    print("\n".join(problems), file=sys.stderr)
    sys.exit(1)
PY
then
  printf 'ok   a recipe-only change reaches the guard locally and in CI\n'
else
  failures=$((failures + 1))
  printf 'FAIL: recipe-only change would not execute the guard\n'
fi

echo '== a missing recipe fails closed =='
cases=$((cases + 1))
if ASSAY_EDITOR_RECIPE="$TMP/does-not-exist.md" bash "$GUARD" >/dev/null 2>&1; then
  failures=$((failures + 1))
  printf 'FAIL: missing recipe was accepted\n'
else
  printf 'ok   missing recipe fails closed\n'
fi

printf '\n'
if [ "$failures" -gt 0 ]; then
  printf 'editor MCP recipe guard self-test: %s of %s case(s) failed\n' "$failures" "$cases"
  exit 1
fi
printf 'editor MCP recipe guard self-test: %s case(s) PASS\n' "$cases"
