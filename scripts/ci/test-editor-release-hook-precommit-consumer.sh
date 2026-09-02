#!/usr/bin/env bash
# Pin editor-mcp-recipe-truth and release-surface through the pre-commit consumer.
#
# The 38/95 suites parse hook YAML themselves and miss last-key-wins duplicates
# (`files: ^$`, `entry: 'true'`). pre-commit/PyYAML keeps the last key, so the
# effective consumer Skips or runs `/usr/bin/true` while those suites stay green.
# This contract owns both hook IDs independently and drives real `pre-commit run`.
set -euo pipefail

# shellcheck source=scripts/ci/lib/clear-git-repository-env.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/clear-git-repository-env.sh"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CONFIG="$ROOT/.pre-commit-config.yaml"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

EDITOR_ID="editor-mcp-recipe-truth"
RELEASE_ID="release-surface"
CONSUMER_ID="editor-release-hook-precommit-consumer"
CONTRACT_REL="scripts/ci/test-editor-release-hook-precommit-consumer.sh"

EDITOR_ENTRY="bash -c 'bash scripts/ci/test-editor-plugin-install-commands.sh && bash scripts/ci/test-check-editor-mcp-recipe-truth.sh && bash scripts/ci/check-editor-mcp-recipe-truth.sh'"
RELEASE_ENTRY="bash -c 'bash scripts/ci/test-check-release-surface.sh && bash scripts/ci/check-release-surface.sh'"
CONSUMER_ENTRY="bash $CONTRACT_REL"

EDITOR_DROP="bash scripts/ci/test-editor-plugin-install-commands.sh && "
RELEASE_DROP="bash scripts/ci/test-check-release-surface.sh && "

failures=0

fail() {
  failures=$((failures + 1))
  printf 'FAIL: %s\n' "$*"
}

ok() { printf 'ok   %s\n' "$*"; }

if ! command -v pre-commit >/dev/null 2>&1; then
  fail "pre-commit not on PATH (CI pin is pre-commit==4.4.0)"
  exit 1
fi
CFG_PY="$TMP/editor_release_hook_precommit_consumer.py"
cat > "$CFG_PY" << 'ENDPY'
# Helper for test-editor-release-hook-precommit-consumer.sh (invoked via python3).
from __future__ import annotations

import re
import sys
from pathlib import Path


def uncommented_items(src: str) -> list[tuple[str, str]]:
    items: list[tuple[str, str]] = []
    for line in src.splitlines():
        code = line.split("#", 1)[0].rstrip()
        if ":" not in code:
            continue
        key, val = code.strip().split(":", 1)
        items.append((key, val.strip()))
    return items


def hook_starts(src: str, hook_id: str) -> list[int]:
    marker = f"      - id: {hook_id}\n"
    starts: list[int] = []
    i = 0
    while True:
        j = src.find(marker, i)
        if j < 0:
            return starts
        starts.append(j)
        i = j + 1


def hook_span(src: str, hook_id: str) -> tuple[int, int]:
    starts = hook_starts(src, hook_id)
    if len(starts) != 1:
        raise SystemExit(
            f"expected exactly one hook block for {hook_id}, found {len(starts)}"
        )
    start = starts[0]
    rest = src[start + 1 :]
    cuts = []
    for token in ("\n      - id:", "\n  - repo:"):
        k = rest.find(token)
        if k >= 0:
            cuts.append(k)
    end = start + 1 + min(cuts) if cuts else len(src)
    return start, end


def hook_block(src: str, hook_id: str) -> str:
    start, end = hook_span(src, hook_id)
    return src[start:end]


def replace_block(src: str, hook_id: str, new_block: str) -> str:
    start, end = hook_span(src, hook_id)
    return src[:start] + new_block.rstrip("\n") + "\n" + src[end:]


def insert_after_key(block: str, key: str, new_line: str) -> str:
    lines = block.splitlines(keepends=True)
    out: list[str] = []
    inserted = False
    for line in lines:
        out.append(line)
        code = line.split("#", 1)[0]
        if not inserted and code.strip().startswith(f"{key}:"):
            indent = line[: len(line) - len(line.lstrip(" "))]
            out.append(f"{indent}{new_line}\n")
            inserted = True
    if not inserted:
        raise SystemExit(f"no uncommented {key}: in hook block")
    return "".join(out)


def problems_for(
    src: str,
    *,
    editor_id: str,
    release_id: str,
    consumer_id: str,
    editor_entry: str,
    release_entry: str,
    consumer_entry: str,
    contract_rel: str,
) -> list[str]:
    problems: list[str] = []
    required = {
        editor_id: {
            "entry": editor_entry,
            "paths": (
                "scripts/ci/test-editor-plugin-install-commands.sh",
                "scripts/ci/test-check-editor-mcp-recipe-truth.sh",
                "scripts/ci/check-editor-mcp-recipe-truth.sh",
                "scripts/ci/lib/editor-plugin-install-commands.sh",
                "docs/guides/editor-mcp-recipe.md",
                contract_rel,
            ),
        },
        release_id: {
            "entry": release_entry,
            "paths": (
                ".github/assay-release-tag",
                "scripts/ci/check-release-surface.sh",
                "scripts/ci/test-check-release-surface.sh",
                "docs/guides/editor-mcp-recipe.md",
                contract_rel,
            ),
        },
    }
    for hook_id, spec in required.items():
        starts = hook_starts(src, hook_id)
        if len(starts) != 1:
            problems.append(
                f"{hook_id}: expected exactly one hook block, found {len(starts)}"
            )
            continue
        block = hook_block(src, hook_id)
        items = uncommented_items(block)
        files_vals = [val for key, val in items if key == "files"]
        entry_vals = [val for key, val in items if key == "entry"]
        if len(files_vals) != 1:
            problems.append(
                f"{hook_id}: expected exactly one uncommented files: selector, found {len(files_vals)}"
            )
        else:
            if files_vals[0] == "^$":
                problems.append(f"{hook_id}: files: selector is ^$")
            else:
                pattern = re.compile(files_vals[0])
                for path in spec["paths"]:
                    if not pattern.search(path):
                        problems.append(
                            f"{hook_id}: files: selector does not match {path}"
                        )
        if len(entry_vals) != 1:
            problems.append(
                f"{hook_id}: expected exactly one uncommented entry:, found {len(entry_vals)}"
            )
        elif entry_vals[0] != spec["entry"]:
            problems.append(f"{hook_id}: entry is not the exact command chain")

    starts = hook_starts(src, consumer_id)
    if len(starts) != 1:
        problems.append(
            f"{consumer_id}: expected exactly one hook block, found {len(starts)}"
        )
        return problems
    block = hook_block(src, consumer_id)
    items = uncommented_items(block)
    keys = [key for key, _ in items]
    entry_vals = [val for key, val in items if key == "entry"]
    files_vals = [val for key, val in items if key == "files"]
    if "always_run" not in keys or "always_run: true" not in block:
        problems.append(f"{consumer_id}: must set always_run: true")
    if "pass_filenames" not in keys or "pass_filenames: false" not in block:
        problems.append(f"{consumer_id}: must set pass_filenames: false")
    if "language" not in keys or "language: system" not in block:
        problems.append(f"{consumer_id}: must set language: system")
    if files_vals:
        problems.append(
            f"{consumer_id}: must not have a files: start-condition regex"
        )
    if len(entry_vals) != 1 or entry_vals[0] != consumer_entry:
        problems.append(f"{consumer_id}: entry must invoke only {contract_rel}")
    return problems


def main() -> None:
    (
        editor_id,
        release_id,
        consumer_id,
        editor_entry,
        release_entry,
        consumer_entry,
        contract_rel,
        action,
        config_path,
        dest,
        *rest
    ) = sys.argv[1:]
    src = Path(config_path).read_text(encoding="utf-8")

    if action == "check":
        problems = problems_for(
            src,
            editor_id=editor_id,
            release_id=release_id,
            consumer_id=consumer_id,
            editor_entry=editor_entry,
            release_entry=release_entry,
            consumer_entry=consumer_entry,
            contract_rel=contract_rel,
        )
        if problems:
            sys.stderr.write("\n".join(problems) + "\n")
            raise SystemExit(1)
        return

    if action == "extract-minimal":
        hook_id = rest[0]
        block = hook_block(src, hook_id)
        Path(dest).write_text(
            'minimum_pre_commit_version: "4.4.0"\n'
            "repos:\n"
            "  - repo: local\n"
            "    hooks:\n" + block.rstrip("\n") + "\n",
            encoding="utf-8",
        )
        return

    if action == "mutate":
        kind, hook_id = rest[0], rest[1]
        block = hook_block(src, hook_id)
        if kind == "second-files":
            block = insert_after_key(block, "files", "files: ^$")
        elif kind == "entry-true":
            block = insert_after_key(block, "entry", "entry: 'true'")
        elif kind == "drop-command":
            token = rest[2]
            if token not in block:
                raise SystemExit(f"drop token not found in {hook_id}: {token!r}")
            block = block.replace(token, "", 1)
        else:
            raise SystemExit(f"unknown mutation {kind}")
        Path(dest).write_text(replace_block(src, hook_id, block), encoding="utf-8")
        return

    raise SystemExit(f"unknown action {action}")


if __name__ == "__main__":
    main()
ENDPY

cfg() {
  python3 "$CFG_PY" \
    "$EDITOR_ID" "$RELEASE_ID" "$CONSUMER_ID" \
    "$EDITOR_ENTRY" "$RELEASE_ENTRY" "$CONSUMER_ENTRY" \
    "$CONTRACT_REL" "$@"
}

check_wiring() {
  local path="$1"
  cfg check "$path" "$path"
}

unclose_plugin_fence() {
  python3 - "$1" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
needle = "# assay-editor-plugin-install-commands:end\n```\n"
if needle not in text:
    raise SystemExit("plugin bash-fence close not found")
path.write_text(
    text.replace(needle, "# assay-editor-plugin-install-commands:end\n", 1),
    encoding="utf-8",
)
PY
}

git_seed() {
  local dest="$1"
  git -C "$dest" init -q
  git -C "$dest" config user.email "ci@example.com"
  git -C "$dest" config user.name "CI"
  git -C "$dest" config commit.gpgsign false
  git -C "$dest" add -A
  git -C "$dest" commit -q -m seed
}

archive_into() {
  local dest="$1"
  shift
  mkdir -p "$dest"
  git -C "$ROOT" archive HEAD -- "$@" | tar -x -C "$dest"
}

prepare_editor_tree() {
  local dest="$1"
  archive_into "$dest" \
    scripts/ci/test-editor-plugin-install-commands.sh \
    scripts/ci/test-check-editor-mcp-recipe-truth.sh \
    scripts/ci/check-editor-mcp-recipe-truth.sh \
    scripts/ci/lib/editor-plugin-install-commands.sh \
    scripts/ci/lib/clear-git-repository-env.sh \
    docs/guides/editor-mcp-recipe.md \
    .github/workflows/kernel-matrix.yml
}

prepare_release_tree() {
  local dest="$1"
  local paths=""
  paths="$(
    git -C "$ROOT" ls-files \
      Cargo.toml Cargo.lock \
      'crates/*/Cargo.toml' assay-python-sdk/Cargo.toml \
      README.md SECURITY.md llms.txt mkdocs.yml \
      docs/getting-started/installation.md \
      docs/getting-started/index.md \
      docs/getting-started/quickstart.md \
      docs/getting-started/ci-integration.md \
      docs/reference/cli/index.md \
      docs/AIcontext/user-flows.md \
      docs/use-cases/ci-gate.md \
      docs/use-cases/air-gapped.md \
      docs/index.md docs/COMMUNITY.md \
      docs/python-sdk/index.md docs/migration-v1.2.md \
      docs/guides/editor-mcp-recipe.md \
      examples/mcp-quickstart/README.md \
      .devcontainer/welcome.sh demo/CODESPACES-PLAYBOOK.md \
      .github/assay-release-tag \
      scripts/ci/check-release-surface.sh \
      scripts/ci/test-check-release-surface.sh \
      scripts/ci/read-assay-release-tag.sh \
      scripts/ci/release_readme.py \
      scripts/ci/lib/editor-plugin-install-commands.sh \
      scripts/ci/lib/clear-git-repository-env.sh
  )"
  # Paths are repo-relative and space-free; keep this split for bash 3.2.
  # shellcheck disable=SC2086
  archive_into "$dest" $paths
}

install_hook_config() {
  local src_config="$1" hook_id="$2" dest="$3"
  cfg extract-minimal "$src_config" "$dest/.pre-commit-config.yaml" "$hook_id"
}

consumer_status() {
  local dest="$1"
  local hook_id="$2"
  local out="$dest/precommit.out"
  set +e
  (
    cd "$dest"
    export PRE_COMMIT_HOME="$dest/pc-home"
    pre-commit run "$hook_id" --color never --all-files
  ) >"$out" 2>&1
  local rc=$?
  set -e
  if grep -Fq 'Skipped' "$out"; then
    printf 'skipped'
    return 0
  fi
  if grep -Fq 'Passed' "$out" && [ "$rc" -eq 0 ]; then
    printf 'passed'
    return 0
  fi
  if [ "$rc" -ne 0 ]; then
    printf 'failed'
    return 0
  fi
  printf 'unknown'
}

expect_wiring_red() {
  local label="$1" config="$2"
  if check_wiring "$config" >"$TMP/$label.wiring" 2>&1; then
    fail "$label — wiring stayed green"
  else
    ok "RED $label"
  fi
}

expect_consumer() {
  local label="$1" dest="$2" hook_id="$3" want="$4"
  local got
  got="$(consumer_status "$dest" "$hook_id")"
  if [ "$got" = "$want" ]; then
    ok "$label (consumer $got)"
  else
    fail "$label — consumer $got, wanted $want"
    sed 's/^/      /' "$dest/precommit.out" || true
  fi
}

echo '== live wiring =='
if check_wiring "$CONFIG"; then
  ok 'live unmutated tree wiring'
else
  fail 'live unmutated tree wiring'
fi

echo '== pin behavior: unmutated consumer must fail closed =='
teeth_editor="$TMP/teeth-editor"
prepare_editor_tree "$teeth_editor"
unclose_plugin_fence "$teeth_editor/docs/guides/editor-mcp-recipe.md"
install_hook_config "$CONFIG" "$EDITOR_ID" "$teeth_editor"
git_seed "$teeth_editor"
expect_consumer 'malformed recipe (missing plugin bash-fence close)' \
  "$teeth_editor" "$EDITOR_ID" failed

teeth_release="$TMP/teeth-release"
prepare_release_tree "$teeth_release"
printf '%s\n' '0.0.0' > "$teeth_release/.github/assay-release-tag"
install_hook_config "$CONFIG" "$RELEASE_ID" "$teeth_release"
git_seed "$teeth_release"
expect_consumer 'stale pin 0.0.0' "$teeth_release" "$RELEASE_ID" failed

echo '== named wiring mutations =='
cfg mutate "$CONFIG" "$TMP/second-files-editor.yaml" second-files "$EDITOR_ID"
expect_wiring_red 'second files: ^$ on editor hook' "$TMP/second-files-editor.yaml"

cfg mutate "$CONFIG" "$TMP/second-files-release.yaml" second-files "$RELEASE_ID"
expect_wiring_red 'second files: ^$ on release-surface hook' "$TMP/second-files-release.yaml"

cfg mutate "$CONFIG" "$TMP/entry-true-editor.yaml" entry-true "$EDITOR_ID"
expect_wiring_red 'entry: true on editor hook' "$TMP/entry-true-editor.yaml"

cfg mutate "$CONFIG" "$TMP/entry-true-release.yaml" entry-true "$RELEASE_ID"
expect_wiring_red 'entry: true on release-surface hook' "$TMP/entry-true-release.yaml"

cfg mutate "$CONFIG" "$TMP/drop-editor.yaml" drop-command "$EDITOR_ID" "$EDITOR_DROP"
expect_wiring_red 'remove test-editor-plugin-install-commands.sh from editor chain' \
  "$TMP/drop-editor.yaml"

cfg mutate "$CONFIG" "$TMP/drop-release.yaml" drop-command "$RELEASE_ID" "$RELEASE_DROP"
expect_wiring_red 'remove test-check-release-surface.sh from release-surface chain' \
  "$TMP/drop-release.yaml"

echo '== malformed recipe paired with editor bypasses =='
pair="$TMP/pair-editor-second-files"
prepare_editor_tree "$pair"
unclose_plugin_fence "$pair/docs/guides/editor-mcp-recipe.md"
install_hook_config "$TMP/second-files-editor.yaml" "$EDITOR_ID" "$pair"
git_seed "$pair"
expect_consumer 'malformed recipe + second files: ^$ on editor hook' \
  "$pair" "$EDITOR_ID" skipped

pair="$TMP/pair-editor-entry-true"
prepare_editor_tree "$pair"
unclose_plugin_fence "$pair/docs/guides/editor-mcp-recipe.md"
install_hook_config "$TMP/entry-true-editor.yaml" "$EDITOR_ID" "$pair"
git_seed "$pair"
expect_consumer 'malformed recipe + entry: true on editor hook' \
  "$pair" "$EDITOR_ID" passed

echo '== stale pin paired with release-surface bypasses =='
pair="$TMP/pair-release-second-files"
prepare_release_tree "$pair"
printf '%s\n' '0.0.0' > "$pair/.github/assay-release-tag"
install_hook_config "$TMP/second-files-release.yaml" "$RELEASE_ID" "$pair"
git_seed "$pair"
expect_consumer 'stale pin 0.0.0 + second files: ^$ on release-surface hook' \
  "$pair" "$RELEASE_ID" skipped

pair="$TMP/pair-release-entry-true"
prepare_release_tree "$pair"
printf '%s\n' '0.0.0' > "$pair/.github/assay-release-tag"
install_hook_config "$TMP/entry-true-release.yaml" "$RELEASE_ID" "$pair"
git_seed "$pair"
expect_consumer 'stale pin 0.0.0 + entry: true on release-surface hook' \
  "$pair" "$RELEASE_ID" passed

printf '\n'
if [ "$failures" -gt 0 ]; then
  printf 'editor/release-surface pre-commit consumer: %s failure(s)\n' "$failures"
  exit 1
fi
printf 'editor/release-surface pre-commit consumer: PASS\n'
