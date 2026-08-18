#!/usr/bin/env bash
# #2515: review-split-wave.sh must inventory dirty unstaged/untracked files.
#
# The Assay Sim usage example is proven to exist on this pin; this test pins that
# fact and does not demand a replacement path.
set -euo pipefail

# shellcheck source=scripts/ci/lib/clear-git-repository-env.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/clear-git-repository-env.sh"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$ROOT/scripts/ci/review-split-wave.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

FAILURES=0
ok()  { echo "ok    $1"; }
bad() { echo "FAIL  $1"; FAILURES=$((FAILURES + 1)); }

assert_usage_examples_exist() {
  local script="$1"
  python3 - "$script" "$ROOT" <<'PY'
import re, subprocess, sys
from pathlib import Path
text = Path(sys.argv[1]).read_text(encoding="utf-8")
root = Path(sys.argv[2])
examples = re.findall(
    r"review-split-wave\.sh\s+\S+\s+'([^']+)'",
    text,
)
if not examples:
    raise SystemExit("no usage examples with an allowed-path regex")
tracked = subprocess.check_output(
    ["git", "-C", str(root), "ls-files"],
    text=True,
).splitlines()
for regex in examples:
    rx = re.compile(regex)
    if not any(rx.search(path) for path in tracked):
        raise SystemExit(f"usage example regex matches no tracked file: {regex}")
print(f"{len(examples)} usage example regexes match tracked files")
PY
}

make_stub_cargo() {
  local bin="$1"
  mkdir -p "$bin"
  cat >"$bin/cargo" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
  chmod +x "$bin/cargo"
}

init_fixture() {
  local repo="$1"
  mkdir -p "$repo/allowed"
  git -C "$repo" init -q
  git -C "$repo" config user.email "ci@example.com"
  git -C "$repo" config user.name "CI"
  printf 'base\n' >"$repo/README"
  printf 'ok\n' >"$repo/allowed/keep.rs"
  git -C "$repo" add README allowed/keep.rs
  git -C "$repo" commit -q -m base
  printf 'changed\n' >"$repo/allowed/keep.rs"
  git -C "$repo" add allowed/keep.rs
  git -C "$repo" commit -q -m change
}

run_review() {
  local repo="$1"
  local script="$2"
  shift 2
  local bin="$TMP/bin"
  make_stub_cargo "$bin"
  (
    cd "$repo"
    PATH="$bin:$PATH"
    bash "$script" demo '^allowed/' HEAD~1 "$@"
  )
}

if examples_out="$(assert_usage_examples_exist "$SCRIPT")"; then
  ok "usage examples resolve on the current tree ($examples_out)"
else
  bad "usage examples: $examples_out"
fi

# Untracked leak must not be silent.
init_fixture "$TMP/untracked"
printf 'leak\n' >"$TMP/untracked/surprise.txt"
if out="$(run_review "$TMP/untracked" "$SCRIPT" 2>&1)"; then
  bad "untracked leak left the gate green"
  printf '%s\n' "$out"
else
  if grep -Fq 'surprise.txt' <<<"$out"; then
    ok "untracked leak is inventoried ($out)"
  else
    bad "untracked leak failed without naming surprise.txt: $out"
  fi
fi

# Unstaged out-of-scope edit must not be silent.
init_fixture "$TMP/unstaged"
printf 'dirty\n' >>"$TMP/unstaged/README"
if out="$(run_review "$TMP/unstaged" "$SCRIPT" 2>&1)"; then
  bad "unstaged leak left the gate green"
  printf '%s\n' "$out"
else
  if grep -Fq 'README' <<<"$out"; then
    ok "unstaged leak is inventoried ($out)"
  else
    bad "unstaged leak failed without naming README: $out"
  fi
fi

# Mutation: restore the two-source inventory and require the leak to go silent.
mutant="$TMP/review-split-wave.mutant.sh"
python3 - "$SCRIPT" "$mutant" <<'PY'
from pathlib import Path
import sys
src = Path(sys.argv[1]).read_text(encoding="utf-8")
old = """changed_files="$(
  {
    git diff --name-only "${base_ref}"...HEAD
    git diff --cached --name-only
  } | sort -u
)\""""
if old not in src:
    # After the one-function extract, strip unstaged/untracked collection.
    mutated = src
    mutated = mutated.replace("git diff --name-only\n", "")
    mutated = mutated.replace("git ls-files --others --exclude-standard\n", "")
else:
    mutated = src
Path(sys.argv[2]).write_text(mutated, encoding="utf-8")
PY
chmod +x "$mutant"
init_fixture "$TMP/mutant"
printf 'leak\n' >"$TMP/mutant/surprise.txt"
if out="$(run_review "$TMP/mutant" "$mutant" 2>&1)"; then
  ok "narrowed inventory mutation stays silent on untracked leak"
else
  bad "narrowed inventory mutation still caught the leak: $out"
fi

if [[ "$FAILURES" -ne 0 ]]; then
  echo "$FAILURES review-split-wave case(s) failed"
  exit 1
fi
echo "PASS: review-split-wave dirty-tree contract"
