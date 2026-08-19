#!/usr/bin/env bash
# #2515: review-split-wave.sh must inventory dirty unstaged/untracked files.
#
# The Assay Sim usage example is proven to exist on this pin; this test pins that
# fact and does not demand a replacement path.
set -euo pipefail

# shellcheck source=scripts/ci/lib/clear-git-repository-env.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/clear-git-repository-env.sh"

_TRUTH_LIB="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/resource_ceilings.py"
python3 "$_TRUTH_LIB" reject-overrides
python3 "$_TRUTH_LIB" assert-reject-caller "${BASH_SOURCE[0]}"
if [[ -n "${REVIEW_SPLIT_ROOT+x}" ]]; then
  echo "FAIL: REVIEW_SPLIT_ROOT cannot replace the script worktree" >&2
  exit 1
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$ROOT/scripts/ci/review-split-wave.sh"
CEILINGS="$ROOT/scripts/ci/lib/resource_ceilings.py"
export PYTHONPATH="$ROOT/scripts/ci/lib${PYTHONPATH:+:$PYTHONPATH}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
unset REVIEW_SPLIT_CEILINGS

FAILURES=0
ok()  { echo "ok    $1"; }
bad() { echo "FAIL  $1"; FAILURES=$((FAILURES + 1)); }

assert_usage_examples_exist() {
  local script="$1"
  python3 - "$script" "$ROOT" "$CEILINGS" <<'PY'
import re, subprocess, sys
from pathlib import Path

from resource_ceilings import read_bounded_file

script = Path(sys.argv[1])
root = Path(sys.argv[2])
ceilings = Path(sys.argv[3])
text = read_bounded_file(str(script)).decode("utf-8")
examples = re.findall(
    r"review-split-wave\.sh\s+\S+\s+'([^']+)'",
    text,
)
if not examples:
    raise SystemExit("no usage examples with an allowed-path regex")
git = subprocess.Popen(
    ["git", "-C", str(root), "ls-files"],
    stdout=subprocess.PIPE,
)
assert git.stdout is not None
tracked = subprocess.check_output(
    [sys.executable, str(ceilings), "inventory"],
    stdin=git.stdout,
    text=True,
).splitlines()
if git.wait() != 0:
    raise SystemExit("git ls-files failed")
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

install_hermetic_review() {
  local dest="$1"
  python3 - "$SCRIPT" "$CEILINGS" "$dest" <<'PY'
import sys
from pathlib import Path

from resource_ceilings import read_bounded_file, require_bounded_bytes

dest = Path(sys.argv[3])
(dest / "lib").mkdir(parents=True, exist_ok=True)
script = read_bounded_file(sys.argv[1])
helper = read_bounded_file(sys.argv[2])
require_bounded_bytes(script, "hermetic review-split-wave")
require_bounded_bytes(helper, "hermetic resource_ceilings")
(dest / "review-split-wave.sh").write_bytes(script)
(dest / "lib" / "resource_ceilings.py").write_bytes(helper)
PY
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
install_hermetic_review "$TMP/narrowed-layout"
mutant="$TMP/narrowed-layout/review-split-wave.sh"
python3 - "$mutant" "$mutant" <<'PY'
from pathlib import Path
import sys

from resource_ceilings import read_bounded_file, require_bounded_bytes

src = read_bounded_file(sys.argv[1]).decode("utf-8")
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
require_bounded_bytes(mutated.encode("utf-8"), "review-split-wave inventory mutant")
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

if out="$(printf 'a\nb\nc\n' | BOUNDED_INVENTORY_MAX_PATHS=2 python3 "$CEILINGS" inventory 2>&1)"; then
  bad "path-count ceiling left the inventory green"
elif grep -Fq 'max path count' <<<"$out"; then
  ok "path-count ceiling turns red ($out)"
else
  bad "path-count ceiling red without max path count: $out"
fi

if out="$(printf 'xxxxxxxxxxxxxxxxxxxx\nyyyyyyyyyyyyyyyyyyyy\n' | BOUNDED_INVENTORY_MAX_BYTES=30 python3 "$CEILINGS" inventory 2>&1)"; then
  bad "inventory byte ceiling left the inventory green"
elif grep -Fq 'max byte budget' <<<"$out"; then
  ok "inventory byte ceiling turns red ($out)"
else
  bad "inventory byte ceiling red without max byte budget: $out"
fi

python3 -c "import sys; sys.stdout.buffer.write(b'x' * 65537)" >"$TMP/oversized.sh"
if out="$(python3 "$CEILINGS" check-file "$TMP/oversized.sh" 2>&1)"; then
  bad "source-script byte ceiling left green"
elif grep -Fq '65536-byte ceiling' <<<"$out"; then
  ok "source-script byte ceiling turns red ($out)"
else
  bad "source-script byte ceiling red without 65536-byte ceiling: $out"
fi

read -r CANON_PATHS CANON_BYTES < <(python3 "$CEILINGS" canonical-inventory-limits)
raise_paths=$((CANON_PATHS + 1))
raise_bytes=$((CANON_BYTES + 1))

if out="$(BOUNDED_INVENTORY_MAX_PATHS="$raise_paths" python3 "$CEILINGS" inventory </dev/null 2>&1)"; then
  bad "caller-raised path cap left the inventory green"
elif grep -Fq "cannot exceed canonical ${CANON_PATHS}" <<<"$out"; then
  ok "caller cannot raise the path cap ($out)"
else
  bad "raised path cap red without canonical ${CANON_PATHS}: $out"
fi

if out="$(BOUNDED_INVENTORY_MAX_BYTES="$raise_bytes" python3 "$CEILINGS" inventory </dev/null 2>&1)"; then
  bad "caller-raised byte cap left the inventory green"
elif grep -Fq "cannot exceed canonical ${CANON_BYTES}" <<<"$out"; then
  ok "caller cannot raise the byte cap ($out)"
else
  bad "raised byte cap red without canonical ${CANON_BYTES}: $out"
fi

if out="$(BOUNDED_INVENTORY_MAX_PATHS=0 python3 "$CEILINGS" inventory </dev/null 2>&1)"; then
  bad "nonpositive path cap left the inventory green"
elif grep -Fq 'must be a positive integer' <<<"$out"; then
  ok "nonpositive path cap turns red ($out)"
else
  bad "nonpositive path cap red without positive integer: $out"
fi

if out="$(BOUNDED_INVENTORY_MAX_PATHS=abc python3 "$CEILINGS" inventory </dev/null 2>&1)"; then
  bad "invalid path cap left the inventory green"
elif grep -Fq 'is not a positive integer' <<<"$out"; then
  ok "invalid path cap turns red ($out)"
else
  bad "invalid path cap red without positive integer: $out"
fi

if out="$(BOUNDED_INVENTORY_MAX_PATHS='' python3 "$CEILINGS" inventory </dev/null 2>&1)"; then
  bad "empty path cap left the inventory green"
elif grep -Fq 'is not a positive integer' <<<"$out"; then
  ok "empty path cap turns red ($out)"
else
  bad "empty path cap red without positive integer: $out"
fi

if out="$(BOUNDED_INVENTORY_MAX_PATHS=+2 python3 "$CEILINGS" inventory </dev/null 2>&1)"; then
  bad "plus-prefixed path cap left the inventory green"
elif grep -Fq 'is not a positive integer' <<<"$out"; then
  ok "plus-prefixed path cap turns red ($out)"
else
  bad "plus-prefixed path cap red without positive integer: $out"
fi

if out="$(BOUNDED_INVENTORY_MAX_PATHS=' 2' python3 "$CEILINGS" inventory </dev/null 2>&1)"; then
  bad "spaced path cap left the inventory green"
elif grep -Fq 'is not a positive integer' <<<"$out"; then
  ok "spaced path cap turns red ($out)"
else
  bad "spaced path cap red without positive integer: $out"
fi

if out="$(BOUNDED_INVENTORY_MAX_PATHS=2.0 python3 "$CEILINGS" inventory </dev/null 2>&1)"; then
  bad "float path cap left the inventory green"
elif grep -Fq 'is not a positive integer' <<<"$out"; then
  ok "float path cap turns red ($out)"
else
  bad "float path cap red without positive integer: $out"
fi

if out="$(BOUNDED_INVENTORY_MAX_PATHS=-1 python3 "$CEILINGS" inventory </dev/null 2>&1)"; then
  bad "negative path cap left the inventory green"
elif grep -Fq 'must be a positive integer' <<<"$out"; then
  ok "negative path cap turns red ($out)"
else
  bad "negative path cap red without positive integer: $out"
fi

if out="$(printf 'a\n' | BOUNDED_INVENTORY_MAX_PATHS="$CANON_PATHS" python3 "$CEILINGS" inventory 2>&1)"; then
  ok "equal path cap is allowed"
else
  bad "equal path cap turned red: $out"
fi

if python3 - "$SCRIPT" "$ROOT/scripts/ci/test-review-split-wave.sh" <<'PY'
import sys

from resource_ceilings import read_bounded_file

text = read_bounded_file(sys.argv[1]).decode("utf-8")
if 'python3 "${_REVIEW_SPLIT_CEILINGS}" inventory' not in text:
    raise SystemExit("review-split-wave does not invoke resource_ceilings inventory")
if "${REVIEW_SPLIT_CEILINGS:-" in text or 'python3 "${REVIEW_SPLIT_CEILINGS}"' in text:
    raise SystemExit("review-split-wave still accepts a caller helper override")
suite = read_bounded_file(sys.argv[2]).decode("utf-8")
if "${" + "PROGRAMME_TRUTH_ROOT:-" in suite or "${" + "REVIEW_SPLIT_ROOT:-" in suite:
    raise SystemExit("review-split-wave tests still accept a caller root override")
if "if [[ -n \"${" + "PROGRAMME_TRUTH_AGENTS+x}\" ]]" in suite:
    raise SystemExit("review-split-wave tests still inlined programme override rejects")
PY
then
  ok "review-split-wave inventories through the bounded helper"
else
  bad "review-split-wave does not invoke resource_ceilings inventory"
fi

init_fixture "$TMP/overflow"
python3 - "$TMP/overflow/allowed" "$CEILINGS" <<'PY'
import sys
from pathlib import Path

import runpy

limits = runpy.run_path(sys.argv[2])
root = Path(sys.argv[1])
# keep.rs is already in the committed-vs-base inventory; add the canonical
# max so the production gate sees one path too many.
for i in range(limits["MAX_INVENTORY_PATHS"]):
    (root / f"overflow-{i}.rs").write_text("x\n", encoding="utf-8")
PY
if out="$(run_review "$TMP/overflow" "$SCRIPT" 2>&1)"; then
  bad "production inventory overflow left the gate green"
elif grep -Fq 'max path count' <<<"$out"; then
  ok "production gate rejects inventory overflow"
else
  bad "production overflow red without max path count: $out"
fi

printf '%s\n' 'import sys' 'for line in sys.stdin:' '    sys.stdout.write(line)' \
  >"$TMP/unbounded_ceilings.py"
if out="$(
  REVIEW_SPLIT_CEILINGS="$TMP/unbounded_ceilings.py" \
  run_review "$TMP/overflow" "$SCRIPT" 2>&1
)"; then
  bad "unbounded REVIEW_SPLIT_CEILINGS left the overflow green"
elif grep -Fq 'cannot replace the canonical inventory helper' <<<"$out"; then
  ok "caller cannot replace the production inventory helper"
elif grep -Fq 'max path count' <<<"$out"; then
  ok "caller helper override is ignored and overflow still fails"
else
  bad "unbounded helper override red without reject/overflow: $out"
fi

install_hermetic_review "$TMP/missing-helper"
rm -f "$TMP/missing-helper/lib/resource_ceilings.py"
if out="$(run_review "$TMP/overflow" "$TMP/missing-helper/review-split-wave.sh" 2>&1)"; then
  bad "missing canonical helper left the gate green"
elif grep -Fq 'canonical inventory helper missing' <<<"$out"; then
  ok "missing canonical helper fails closed"
else
  bad "missing helper red without fail-closed: $out"
fi

install_hermetic_review "$TMP/sort-u-layout"
bypass="$TMP/sort-u-layout/review-split-wave.sh"
python3 - "$bypass" "$bypass" <<'PY'
import sys
from pathlib import Path

from resource_ceilings import read_bounded_file, require_bounded_bytes

src = read_bounded_file(sys.argv[1]).decode("utf-8")
old = 'python3 "${_REVIEW_SPLIT_CEILINGS}" inventory'
if old not in src:
    raise SystemExit("production inventory helper invocation missing")
out = src.replace(old, "sort -u")
require_bounded_bytes(out.encode("utf-8"), "review-split-wave sort -u mutant")
Path(sys.argv[2]).write_text(out, encoding="utf-8")
PY
chmod +x "$bypass"
if out="$(run_review "$TMP/overflow" "$bypass" 2>&1)"; then
  ok "sort -u mutant leaks overflow past the helper"
else
  bad "sort -u mutant still enforced a ceiling: $out"
fi

if [[ "$FAILURES" -ne 0 ]]; then
  echo "$FAILURES review-split-wave case(s) failed"
  exit 1
fi
echo "PASS: review-split-wave dirty-tree contract"
