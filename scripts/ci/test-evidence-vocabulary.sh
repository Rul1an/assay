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
import io
import sys
from contextlib import redirect_stdout
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

guard_paths = (
    "scripts/ci/check-evidence-vocabulary.py",
    "scripts/ci/test-evidence-vocabulary.sh",
)
if getattr(module, "SCAN_PATH_EXCLUDES", None) != guard_paths:
    raise SystemExit(
        f"FAIL: SCAN_PATH_EXCLUDES must be exactly {guard_paths}, "
        f"got {getattr(module, 'SCAN_PATH_EXCLUDES', None)!r}"
    )
prefixes = getattr(module, "SCAN_PREFIX_EXCLUDES", ())
if any(prefix == "scripts/ci/" or prefix.startswith("scripts/ci/") for prefix in prefixes):
    raise SystemExit("FAIL: scripts/ci/ must not be a directory-wide scan exclude")
for rel in allow:
    if rel in guard_paths:
        raise SystemExit(f"FAIL: guard path {rel} must not be in ALLOWED_MERKLE_USES")
    if "verify_side_effects.rs" in rel:
        raise SystemExit(
            "FAIL: verify_side_effects.rs must not have an allowlist or TEMPORARY_DEBT exception"
        )
if getattr(module, "TEMPORARY_DEBT", None):
    raise SystemExit("FAIL: TEMPORARY_DEBT must be empty; reserved file stays a live finding")

print("ok: imported ALLOWED_MERKLE_USES is non-vacuous on this tree")
print("ok: scan excludes only the two guard paths")

buf = io.StringIO()
with redirect_stdout(buf):
    rc = module.check_tree(root, allow)
out = buf.getvalue()
if rc == 0:
    raise SystemExit("FAIL: live checker must stay RED while verify_side_effects is uncorrected")
if "verify_side_effects.rs" not in out:
    raise SystemExit(
        "FAIL: live checker RED for some other reason; reserved file was not reported:\n"
        + out
    )
print("ok: live checker RED on reserved verify_side_effects.rs")
PY

# Sibling under scripts/ci/ is still an outward claim. The two guard paths are not.
SIBLING="$TMP/sibling"
init_fixture "$SIBLING"
mkdir -p "$SIBLING/scripts/ci"
printf '%s\n' 'Merkle inclusion' > "$SIBLING/scripts/ci/check-evidence-vocabulary.py"
printf '%s\n' 'Merkle inclusion' > "$SIBLING/scripts/ci/test-evidence-vocabulary.sh"
printf '%s\n' 'Merkle root in a sibling product file' > "$SIBLING/scripts/ci/sibling-product.md"
git -C "$SIBLING" add -A -- scripts/ci/check-evidence-vocabulary.py \
  scripts/ci/test-evidence-vocabulary.sh scripts/ci/sibling-product.md
python3 - "$CHECKER" "$SIBLING" <<'PY'
import importlib.util
import io
import sys
from contextlib import redirect_stdout
from pathlib import Path

checker, root = Path(sys.argv[1]), Path(sys.argv[2])
spec = importlib.util.spec_from_file_location("evidence_vocabulary", checker)
module = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(module)
rekor = {
    "crates/assay-registry/src/rekor.rs": module.ALLOWED_MERKLE_USES[
        "crates/assay-registry/src/rekor.rs"
    ]
}
buf = io.StringIO()
with redirect_stdout(buf):
    rc = module.check_tree(root, rekor)
out = buf.getvalue()
if rc == 0:
    raise SystemExit("FAIL: sibling product file under scripts/ci/ was not flagged")
if "sibling-product.md" not in out:
    raise SystemExit("FAIL: expected sibling-product.md finding, got:\n" + out)
if "check-evidence-vocabulary.py" in out or "test-evidence-vocabulary.sh" in out:
    raise SystemExit("FAIL: guard implementation paths were scanned:\n" + out)
print("ok: sibling product file still fails; only exact guard paths are excluded")
PY

echo "ok: evidence-vocabulary mutations"
