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
# Piggyback: contains an allowlisted phrase but is not FALSE_CLAIM_RE.
_PIGGY_PREFIX='run_root provides a '
_PIGGY_SUFFIX='Merkle inclusion proof.'
PIGGYBACK="${_PIGGY_PREFIX}${_PIGGY_SUFFIX}"
_REKOR_PIGGY_PREFIX='run_root provides '
_REKOR_PIGGY_SUFFIX='Merkle inclusion for the bundle.'
REKOR_PIGGYBACK="${_REKOR_PIGGY_PREFIX}${_REKOR_PIGGY_SUFFIX}"
# Broad .*rfc6962_root.* matches comments, not only the call.
_WILDCARD_PREFIX='// rfc6962_root makes run_root a '
_WILDCARD_SUFFIX='Merkle commitment.'
WILDCARD_REKOR="${_WILDCARD_PREFIX}${_WILDCARD_SUFFIX}"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

init_fixture() {
  local dest="$1"
  mkdir -p "$dest/docs/lint" "$dest/crates/assay-registry/src"
  cat > "$dest/docs/lint/index.md" <<'DOC'
Changing bundle content changes its content hashes.
DOC
  cat > "$dest/crates/assay-registry/src/rekor.rs" <<'DOC'
use checkpoint::{b64, parse_checkpoint, rfc6962_root, sha256};
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
raise SystemExit(module.check_tree(root, chosen, identifiers={}))
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

# Substring allowlist must not pass a longer false claim (the #2222 piggyback).
SPEC_PIGGY="$TMP/spec-piggy"
init_fixture "$SPEC_PIGGY"
mkdir -p "$SPEC_PIGGY/docs/architecture"
printf '%s\n' "$PIGGYBACK" > "$SPEC_PIGGY/docs/architecture/SPEC-Outward-Product-Truth-v1.md"
git -C "$SPEC_PIGGY" add -A -- docs/architecture/SPEC-Outward-Product-Truth-v1.md
python3 - "$CHECKER" "$SPEC_PIGGY" <<'PY'
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
chosen = {
    "docs/architecture/SPEC-Outward-Product-Truth-v1.md": (r"Merkle inclusion proof",),
    "crates/assay-registry/src/rekor.rs": module.ALLOWED_MERKLE_USES[
        "crates/assay-registry/src/rekor.rs"
    ],
}
buf = io.StringIO()
with redirect_stdout(buf):
    rc = module.check_tree(root, chosen, identifiers={})
out = buf.getvalue()
if rc == 0:
    raise SystemExit(
        "FAIL: piggyback-spec expected fail, checker passed (false-green):\n" + out
    )
if "SPEC-Outward-Product-Truth-v1.md" not in out:
    raise SystemExit("FAIL: piggyback-spec did not name the SPEC path:\n" + out)
print("ok: piggyback-spec")
PY

printf '%s\n' "$REKOR_PIGGYBACK" >> "$FIXTURE/crates/assay-registry/src/rekor.rs"
git -C "$FIXTURE" add -A -- crates/assay-registry/src/rekor.rs
python3 - "$CHECKER" "$FIXTURE" <<'PY'
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
    rc = module.check_tree(root, rekor, identifiers={})
out = buf.getvalue()
if rc == 0:
    raise SystemExit(
        "FAIL: piggyback-rekor expected fail, checker passed (false-green):\n" + out
    )
if "rekor.rs" not in out:
    raise SystemExit("FAIL: piggyback-rekor did not name rekor.rs:\n" + out)
print("ok: piggyback-rekor")
PY
cat > "$FIXTURE/crates/assay-registry/src/rekor.rs" <<'DOC'
use checkpoint::{b64, parse_checkpoint, rfc6962_root, sha256};
    // (5) Merkle inclusion: leaf = SHA256(0x00 || canonicalizedBody); recompute the root.
    let Some(recomputed) = rfc6962_root(leaf_hash, ip_index, checkpoint.tree_size, &proof_hashes)
DOC
git -C "$FIXTURE" add -A -- crates/assay-registry/src/rekor.rs

WILDCARD="$TMP/wildcard-rekor"
init_fixture "$WILDCARD"
printf '%s\n' "$WILDCARD_REKOR" >> "$WILDCARD/crates/assay-registry/src/rekor.rs"
git -C "$WILDCARD" add -A -- crates/assay-registry/src/rekor.rs
python3 - "$CHECKER" "$WILDCARD" <<'PY'
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
    rc = module.check_tree(root, rekor, identifiers={})
out = buf.getvalue()
if rc == 0:
    raise SystemExit(
        "FAIL: wildcard-rekor expected fail, checker passed (false-green):\n" + out
    )
if "rekor.rs" not in out:
    raise SystemExit("FAIL: wildcard-rekor did not name rekor.rs:\n" + out)
print("ok: wildcard-rekor")
PY

# Withdrawn experiment metric labels are not Merkle tokens; they still teach a
# production inclusion proof that run_root does not have.
WITHDRAWN="$TMP/withdrawn-metric"
init_fixture "$WITHDRAWN"
mkdir -p "$WITHDRAWN/crates/assay-evidence/tests" \
  "$WITHDRAWN/docs/experiments/evidence-mutation-cost-2026-06/results"
printf '%s\n' 'fn inclusion_proof_hashes(n: u64) -> u32 { n as u32 }' \
  > "$WITHDRAWN/crates/assay-evidence/tests/e3_verify_cost_curve.rs"
printf '%s\n' '| events | inclusion-proof hashes |' \
  > "$WITHDRAWN/docs/experiments/evidence-mutation-cost-2026-06/results/cost.md"
git -C "$WITHDRAWN" add -A -- \
  crates/assay-evidence/tests/e3_verify_cost_curve.rs \
  docs/experiments/evidence-mutation-cost-2026-06/results/cost.md
python3 - "$CHECKER" "$WITHDRAWN" <<'PY'
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
    rc = module.check_tree(root, rekor, identifiers={})
out = buf.getvalue()
if rc == 0:
    raise SystemExit(
        "FAIL: withdrawn-metric expected fail, checker passed (false-green):\n" + out
    )
if "inclusion_proof_hashes" not in out:
    raise SystemExit("FAIL: withdrawn-metric did not flag snake_case label:\n" + out)
if "inclusion-proof hashes" not in out:
    raise SystemExit("FAIL: withdrawn-metric did not flag rendered label:\n" + out)
if "rekor.rs" in out and "withdrawn metric label" in out:
    raise SystemExit("FAIL: genuine Rekor path was flagged as a withdrawn label:\n" + out)
print("ok: withdrawn-metric")
PY

NEWPATH="$TMP/withdrawn-new-path"
init_fixture "$NEWPATH"
mkdir -p "$NEWPATH/docs/experiments/evidence-mutation-cost-2026-06/results"
printf '%s\n' '| events | inclusion-proof hashes |' \
  > "$NEWPATH/docs/experiments/evidence-mutation-cost-2026-06/results/cost-v2.md"
git -C "$NEWPATH" add -A -- \
  docs/experiments/evidence-mutation-cost-2026-06/results/cost-v2.md
python3 - "$CHECKER" "$NEWPATH" <<'PY'
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
    rc = module.check_tree(root, rekor, identifiers={})
out = buf.getvalue()
if rc == 0:
    raise SystemExit(
        "FAIL: withdrawn-new-path expected fail, checker passed (false-green):\n" + out
    )
if "cost-v2.md" not in out or "inclusion-proof hashes" not in out:
    raise SystemExit("FAIL: withdrawn-new-path did not flag cost-v2.md:\n" + out)
if "rekor.rs" in out and "withdrawn metric label" in out:
    raise SystemExit("FAIL: genuine Rekor path was flagged as a withdrawn label:\n" + out)
if not module.is_withdrawn_surface(
    "docs/experiments/evidence-mutation-cost-2026-06/results/cost-v2.md"
):
    raise SystemExit("FAIL: is_withdrawn_surface must cover an unlisted result path")
if module.is_withdrawn_surface("crates/assay-registry/src/rekor.rs"):
    raise SystemExit("FAIL: is_withdrawn_surface must not cover genuine Rekor")
print("ok: withdrawn-new-path")
PY

CASEVAR="$TMP/withdrawn-case"
init_fixture "$CASEVAR"
mkdir -p "$CASEVAR/docs/experiments/evidence-mutation-cost-2026-06/results"
printf '%s\n' '| events | Inclusion-Proof Hashes |' \
  > "$CASEVAR/docs/experiments/evidence-mutation-cost-2026-06/results/cost.md"
git -C "$CASEVAR" add -A -- \
  docs/experiments/evidence-mutation-cost-2026-06/results/cost.md
python3 - "$CHECKER" "$CASEVAR" <<'PY'
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
    rc = module.check_tree(root, rekor, identifiers={})
out = buf.getvalue()
if rc == 0:
    raise SystemExit(
        "FAIL: withdrawn-case expected fail, checker passed (false-green):\n" + out
    )
if "Inclusion-Proof Hashes" not in out and "inclusion-proof hashes" not in out:
    raise SystemExit("FAIL: withdrawn-case did not flag case variation:\n" + out)
if "rekor.rs" in out and "withdrawn metric label" in out:
    raise SystemExit("FAIL: genuine Rekor path was flagged as a withdrawn label:\n" + out)
print("ok: withdrawn-case")
PY

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
import inspect
import io
import re
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
        raise SystemExit("FAIL: verify_side_effects.rs must not have an allowlist exception")
    for pat in allow[rel]:
        if rel == "crates/assay-ebpf/src/vmlinux.rs":
            if pat != r".*merkle_tree_.*":
                raise SystemExit(f"FAIL: vmlinux permit must stay generated-id only, got {pat!r}")
            continue
        if ".*" in pat:
            raise SystemExit(
                f"FAIL: hand-written path {rel} has a prose-capable wildcard: {pat!r}"
            )
if getattr(module, "TEMPORARY_DEBT", None):
    raise SystemExit("FAIL: TEMPORARY_DEBT must not exist; no reserved false-claim exception")
withdrawn_labels = getattr(module, "WITHDRAWN_METRIC_LABELS", None)
if withdrawn_labels != ("inclusion_proof_hashes", "inclusion-proof hashes"):
    raise SystemExit(f"FAIL: WITHDRAWN_METRIC_LABELS drifted: {withdrawn_labels!r}")
if not callable(getattr(module, "is_withdrawn_surface", None)):
    raise SystemExit("FAIL: is_withdrawn_surface must be the shared path predicate")
if getattr(module, "WITHDRAWN_SURFACES", None) is not None:
    raise SystemExit("FAIL: WITHDRAWN_SURFACES filename list must not return")
if not module.is_withdrawn_surface("crates/assay-evidence/tests/e3_verify_cost_curve.rs"):
    raise SystemExit("FAIL: is_withdrawn_surface must cover the Rust harness")
if not module.is_withdrawn_surface(
    "docs/experiments/evidence-mutation-cost-2026-06/results/cost-v2.md"
):
    raise SystemExit("FAIL: is_withdrawn_surface must cover new experiment result paths")
if module.is_withdrawn_surface("crates/assay-registry/src/rekor.rs"):
    raise SystemExit("FAIL: is_withdrawn_surface must not cover genuine Rekor")
label_res = getattr(module, "WITHDRAWN_LABEL_RES", None)
if not label_res or any(not cre.flags & re.IGNORECASE for cre in label_res):
    raise SystemExit("FAIL: WITHDRAWN_LABEL_RES must be case-insensitive")

legacy = getattr(module, "LEGACY_IDENTIFIERS", None)
if not isinstance(legacy, dict) or not legacy:
    raise SystemExit("FAIL: LEGACY_IDENTIFIERS must be a non-empty path-bound dict")
if set(legacy) & set(allow):
    raise SystemExit("FAIL: LEGACY_IDENTIFIERS must not mix with ALLOWED_MERKLE_USES")
stale_legacy = module.allowlist_staleness(root, legacy)
if stale_legacy:
    print("\n".join(stale_legacy), file=sys.stderr)
    raise SystemExit("FAIL: LEGACY_IDENTIFIERS has vacuous or missing entries")
for rel, pats in legacy.items():
    if not rel.startswith("demo/"):
        raise SystemExit(f"FAIL: unexpected legacy-identifier path {rel}")
    for pat in pats:
        if "merkle-chain" not in pat.replace("\\", ""):
            raise SystemExit(f"FAIL: legacy identifier must be an exact merkle-chain filename: {pat!r}")

allowed_src = inspect.getsource(module.line_is_allowed)
if "fullmatch" not in inspect.getsource(module.line_matches):
    raise SystemExit("FAIL: line_matches must use fullmatch")
if ".search(" in inspect.getsource(module.line_matches):
    raise SystemExit("FAIL: line_matches must not substring-search")
if "line_matches" not in allowed_src:
    raise SystemExit("FAIL: line_is_allowed must call line_matches")
stale_src = inspect.getsource(module.allowlist_staleness)
if "line_matches" not in stale_src:
    raise SystemExit("FAIL: staleness must use the same line_matches rule")

print("ok: imported ALLOWED_MERKLE_USES is non-vacuous on this tree")
print("ok: scan excludes only the two guard paths")

buf = io.StringIO()
with redirect_stdout(buf):
    rc = module.check_tree(root, allow)
out = buf.getvalue()
findings = [line for line in out.splitlines() if ": unapproved Merkle claim:" in line or ": false run_root-as-Merkle claim:" in line]
if rc != 0 or findings:
    raise SystemExit(
        "FAIL: live checker must exit 0 with no product findings; got "
        f"rc={rc} findings={len(findings)}:\n" + out
    )
print("ok: live checker GREEN")
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
    rc = module.check_tree(root, rekor, identifiers={})
out = buf.getvalue()
if rc == 0:
    raise SystemExit("FAIL: sibling product file under scripts/ci/ was not flagged")
if "sibling-product.md" not in out:
    raise SystemExit("FAIL: expected sibling-product.md finding, got:\n" + out)
if "check-evidence-vocabulary.py" in out or "test-evidence-vocabulary.sh" in out:
    raise SystemExit("FAIL: guard implementation paths were scanned:\n" + out)
print("ok: sibling product file still fails; only exact guard paths are excluded")
PY

IDENT="$TMP/ident"
init_fixture "$IDENT"
mkdir -p "$IDENT/demo/scenes"
printf '%s\n' 'vhs demo/scenes/merkle-chain.tape' \
  'cp demo/scenes/merkle-chain.mp4 "$TEMP_DIR/shot05.mp4"' \
  > "$IDENT/demo/produce_video.sh"
printf '%s\n' 'Output demo/scenes/merkle-chain.mp4' > "$IDENT/demo/scenes/merkle-chain.tape"
git -C "$IDENT" add -A -- demo/produce_video.sh demo/scenes/merkle-chain.tape
python3 - "$CHECKER" "$IDENT" <<'PY'
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
    rc = module.check_tree(root, rekor, identifiers=module.LEGACY_IDENTIFIERS)
if rc != 0:
    raise SystemExit("FAIL: exact merkle-chain filename lines must not be product findings:\n" + buf.getvalue())

vacuous = {"demo/produce_video.sh": (r"this-pattern-matches-nothing-zz",)}
buf = io.StringIO()
with redirect_stdout(buf):
    rc = module.check_tree(root, rekor, identifiers=vacuous)
if rc == 0:
    raise SystemExit("FAIL: vacuous LEGACY_IDENTIFIERS entry must fail")

(root / "demo/produce_video.sh").write_text(
    (root / "demo/produce_video.sh").read_text() + "Merkle root in narration\n"
)
import subprocess
subprocess.run(["git", "add", "-A", "--", "demo/produce_video.sh"], cwd=root, check=True)
buf = io.StringIO()
with redirect_stdout(buf):
    rc = module.check_tree(root, rekor, identifiers=module.LEGACY_IDENTIFIERS)
out = buf.getvalue()
if rc == 0 or "produce_video.sh" not in out or "Merkle root in narration" not in out:
    raise SystemExit(
        "FAIL: filename identifiers must not mask a claim in the same file:\n" + out
    )
print("ok: merkle-chain filename identifiers are not claims")
PY

python3 - "$ROOT/.pre-commit-config.yaml" "$CHECKER" "$TMP" "$FALSE_INJECT" <<'PY'
import subprocess
import sys
from pathlib import Path

config_path, checker, tmp, false_inject = sys.argv[1:]
text = Path(config_path).read_text()
marker = "      - id: evidence-vocabulary\n"
start = text.find(marker)
if start < 0:
    raise SystemExit("FAIL: evidence-vocabulary hook missing from .pre-commit-config.yaml")
rest = text[start + 1 :]
nxt = rest.find("\n      - id:")
block = text[start : start + 1 + nxt] if nxt >= 0 else text[start:]

def uncommented_keys(src: str) -> list[str]:
    keys = []
    for line in src.splitlines():
        code = line.split("#", 1)[0].rstrip()
        if ":" in code:
            keys.append(code.strip().split(":", 1)[0])
    return keys

keys = uncommented_keys(block)
if "files" in keys:
    raise SystemExit(
        "FAIL: evidence-vocabulary must not have a files: start-condition regex"
    )
if "pass_filenames" not in keys or "pass_filenames: false" not in block:
    raise SystemExit("FAIL: evidence-vocabulary must set pass_filenames: false")
if "always_run" not in keys or "always_run: true" not in block:
    raise SystemExit("FAIL: evidence-vocabulary must set always_run: true")

dest = Path(tmp) / "hook-new-page"
dest.mkdir()
(dest / "scripts" / "ci").mkdir(parents=True)
(dest / "docs").mkdir()
(dest / "scripts" / "ci" / "check-evidence-vocabulary.py").write_bytes(Path(checker).read_bytes())
(dest / ".pre-commit-config.yaml").write_text(
    "repos:\n  - repo: local\n    hooks:\n" + block.rstrip() + "\n"
)
(dest / "docs" / "new-page.md").write_text(false_inject + "\n")
subprocess.run(["git", "init", "-q"], cwd=dest, check=True)
subprocess.run(
    ["git", "add", "-A", "--", "docs/new-page.md", "scripts/ci/check-evidence-vocabulary.py"],
    cwd=dest,
    check=True,
)
proc = subprocess.run(
    ["pre-commit", "run", "evidence-vocabulary", "--color", "never"],
    cwd=dest,
    check=False,
    capture_output=True,
    text=True,
)
out = proc.stdout + proc.stderr
if proc.returncode == 0:
    raise SystemExit(
        "FAIL: real evidence-vocabulary hook did not fail on staged docs/new-page.md:\n"
        + out
    )
if "new-page.md" not in out:
    raise SystemExit(
        "FAIL: hook failed but did not report docs/new-page.md (may have been skipped):\n"
        + out
    )
print("ok: real pre-commit hook fails on a new unlisted docs/new-page.md")
PY

echo "ok: evidence-vocabulary mutations"
