#!/usr/bin/env bash
# Mutation tests for the evidence-vocabulary guard.
# Imports ALLOWED_MERKLE_USES from the checker (one-rule-one-function).
set -euo pipefail

# shellcheck source=scripts/ci/lib/clear-git-repository-env.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/clear-git-repository-env.sh"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECKER="${ROOT}/scripts/ci/check-evidence-vocabulary.py"

if [[ ! -f "$CHECKER" ]]; then
  echo "FAIL: checker does not exist: $CHECKER" >&2
  exit 1
fi

# Keep the two tokens on separate lines so this file is not itself a false claim.
_FALSE_PREFIX='run_root is a '
_FALSE_SUFFIX='Merkle root'
FALSE_INJECT="${_FALSE_PREFIX}${_FALSE_SUFFIX}"
FALSE_INJECT_LOWER="$(printf '%s' "$FALSE_INJECT" | tr '[:upper:]' '[:lower:]')"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

init_fixture() {
  local dest="$1"
  mkdir -p "$dest/docs/lint" "$dest/crates/assay-registry/src"
  cat > "$dest/docs/lint/index.md" <<'DOC'
Changing bundle content changes its content hashes.
DOC
  cat > "$dest/crates/assay-registry/src/rekor.rs" <<'DOC'
    // (5) Merkle inclusion: leaf = SHA256(0x00 || canonicalizedBody); recompute the root.
    let Some(recomputed) = rfc6962_root(leaf_hash, ip_index, checkpoint.tree_size, &proof_hashes)
DOC
  git -C "$dest" init -q
  git -C "$dest" add -A -- docs/lint/index.md crates/assay-registry/src/rekor.rs
  git -C "$dest" -c user.email=test@example.com -c user.name=test \
    -c core.hooksPath=/dev/null commit -q -m fixture
}

run_case() {
  local name="$1"
  local expect="$2"
  local mode="$3"
  local out rc
  set +e
  out="$(
    FALSE_INJECT="$FALSE_INJECT" python3 - "$CHECKER" "$FIXTURE" "$mode" <<'PY'
import importlib.util
import os
import sys
from pathlib import Path

checker, root, mode = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
spec = importlib.util.spec_from_file_location("evidence_vocabulary", checker)
module = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(module)
allow = module.ALLOWED_MERKLE_USES
rekor = {"crates/assay-registry/src/rekor.rs": allow["crates/assay-registry/src/rekor.rs"]}
if mode == "rekor":
    chosen = rekor
elif mode == "vacuous":
    chosen = {
        **rekor,
        "docs/lint/index.md": (r"this-pattern-matches-nothing-zz",),
    }
elif mode == "mask":
    chosen = {**rekor, "docs/lint/index.md": (r"Merkle root",)}
elif mode == "empty":
    chosen = {}
elif mode == "missing-path":
    chosen = {**rekor, "docs/does-not-exist.md": (r"Merkle inclusion",)}
else:
    raise SystemExit(f"unknown mode {mode}")
raise SystemExit(module.check_tree(root, chosen))
PY
  )"
  rc=$?
  set -e
  if [[ "$expect" == pass ]]; then
    if [[ "$rc" -ne 0 ]]; then
      echo "FAIL: $name expected pass, got $rc" >&2
      printf '%s\n' "$out" >&2
      exit 1
    fi
  else
    if [[ "$rc" -eq 0 ]]; then
      echo "FAIL: $name expected fail, got 0" >&2
      printf '%s\n' "$out" >&2
      exit 1
    fi
  fi
}

reset_lint() {
  cat > "$FIXTURE/docs/lint/index.md" <<'DOC'
Changing bundle content changes its content hashes.
DOC
  git -C "$FIXTURE" add -A -- docs/lint/index.md
}

FIXTURE="$TMP/fixture"
init_fixture "$FIXTURE"

run_case baseline pass rekor
echo "ok: baseline"

printf '%s\n' "$FALSE_INJECT" >> "$FIXTURE/docs/lint/index.md"
git -C "$FIXTURE" add -A -- docs/lint/index.md
run_case false-run-root-merkle fail rekor
echo "ok: false-run-root-merkle"

reset_lint
printf '%s\n' "$FALSE_INJECT_LOWER" >> "$FIXTURE/docs/lint/index.md"
git -C "$FIXTURE" add -A -- docs/lint/index.md
run_case lowercase-false-run-root-merkle fail rekor
echo "ok: lowercase-false-run-root-merkle"

reset_lint
run_case genuine-rekor-merkle pass rekor
echo "ok: genuine-rekor-merkle"

run_case vacuous-allowlist-entry fail vacuous
echo "ok: vacuous-allowlist-entry"

printf '%s\n' "$FALSE_INJECT" >> "$FIXTURE/docs/lint/index.md"
git -C "$FIXTURE" add -A -- docs/lint/index.md
run_case allowlist-does-not-mask-claim fail mask
echo "ok: allowlist-does-not-mask-claim"

reset_lint
run_case empty-allowlist fail empty
echo "ok: empty-allowlist"

run_case missing-allowlisted-path fail missing-path
echo "ok: missing-allowlisted-path"

printf 'Merkle\0root' > "$FIXTURE/binary.bin"
git -C "$FIXTURE" add -A -- binary.bin
run_case binary-input pass rekor
echo "ok: binary-input"

python3 - "$CHECKER" "$ROOT" <<'PY'
import importlib.util
import sys
from pathlib import Path

checker, root = Path(sys.argv[1]), Path(sys.argv[2])
spec = importlib.util.spec_from_file_location("evidence_vocabulary", checker)
module = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(module)
allow = module.ALLOWED_MERKLE_USES
if allow is not module.ALLOWED_MERKLE_USES:
    raise SystemExit("FAIL: test did not import the checker's allowlist dict")
if not allow:
    raise SystemExit("FAIL: production allowlist is empty")
stale = module.allowlist_staleness(root, allow)
if stale:
    print("\n".join(stale), file=sys.stderr)
    raise SystemExit("FAIL: production allowlist has vacuous or missing entries")
print("ok: imported ALLOWED_MERKLE_USES is non-vacuous on this tree")
PY

echo "ok: evidence-vocabulary mutations"
