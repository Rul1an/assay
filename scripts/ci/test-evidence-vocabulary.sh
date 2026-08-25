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
run_case binary-input fail rekor
echo "ok: binary-input-nul-without-magic"

# Hostile GIT_DIR pointing at an empty repo must not pass with zero files.
HOSTILE="$TMP/hostile-gitdir"
init_fixture "$HOSTILE"
printf '%s\n' "$FALSE_INJECT" >> "$HOSTILE/docs/lint/index.md"
git -C "$HOSTILE" add -A -- docs/lint/index.md
EMPTY_GIT="$TMP/empty-repo"
git init -q "$EMPTY_GIT"
python3 - "$CHECKER" "$HOSTILE" "$EMPTY_GIT" <<'PY'
import importlib.util
import io
import os
import sys
from contextlib import redirect_stdout
from pathlib import Path

checker, root, empty = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
spec = importlib.util.spec_from_file_location("evidence_vocabulary", checker)
module = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(module)
os.environ["GIT_DIR"] = str(empty / ".git")
os.environ["GIT_WORK_TREE"] = str(empty)
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
    raise SystemExit("FAIL: hostile-gitdir expected fail, checker passed:\n" + out)
if "evidence-vocabulary=passed" in out:
    raise SystemExit("FAIL: hostile-gitdir printed passed:\n" + out)
print("ok: hostile-gitdir")
PY

EMPTY_TRACKED="$TMP/empty-tracked"
git init -q "$EMPTY_TRACKED"
python3 - "$CHECKER" "$EMPTY_TRACKED" <<'PY'
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
buf = io.StringIO()
with redirect_stdout(buf):
    rc = module.check_tree(root, {}, identifiers={})
out = buf.getvalue()
if rc == 0 or "tracked set is empty" not in out:
    raise SystemExit("FAIL: empty tracked set must fail closed:\n" + out)
print("ok: empty-tracked-set")
PY

# NUL + false claim in docs/*.md must fail closed. NUL without recognized
# binary magic fails regardless of extension; genuine magic-matched binaries skip.
NULDOC="$TMP/nul-docs"
init_fixture "$NULDOC"
python3 - "$CHECKER" "$NULDOC" "$FALSE_INJECT" <<'PY'
import importlib.util
import io
import subprocess
import sys
from contextlib import redirect_stdout
from pathlib import Path

checker, root, false_inject = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
hidden = root / "docs" / "hidden.md"
hidden.write_bytes((false_inject + "\n").encode() + b"\x00hidden\n")
subprocess.run(["git", "add", "-A", "--", "docs/hidden.md"], cwd=root, check=True)
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
if rc == 0 or "hidden.md" not in out:
    raise SystemExit("FAIL: NUL docs/*.md must fail closed:\n" + out)
print("ok: nul-docs-md")
PY

# vmlinux prose piggyback on merkle_tree_ must fail.
VMLINUX="$TMP/vmlinux-piggy"
init_fixture "$VMLINUX"
mkdir -p "$VMLINUX/crates/assay-ebpf/src"
python3 - "$CHECKER" "$VMLINUX" <<'PY'
import importlib.util
import io
import subprocess
import sys
from contextlib import redirect_stdout
from pathlib import Path

checker, root = Path(sys.argv[1]), Path(sys.argv[2])
vmlinux = root / "crates/assay-ebpf/src/vmlinux.rs"
vmlinux.write_text(
    "\n".join(
        [
            "    pub read_merkle_tree_page: ::core::option::Option<",
            "    pub write_merkle_tree_block: ::core::option::Option<",
            "pub struct merkle_tree_params {",
            "    pub tree_params: merkle_tree_params,",
            "// merkle_tree_probe proves every evidence digest is a Merkle root",
            "",
        ]
    )
)
subprocess.run(
    ["git", "add", "-A", "--", "crates/assay-ebpf/src/vmlinux.rs"],
    cwd=root,
    check=True,
)
spec = importlib.util.spec_from_file_location("evidence_vocabulary", checker)
module = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(module)
chosen = {
    "crates/assay-ebpf/src/vmlinux.rs": module.ALLOWED_MERKLE_USES[
        "crates/assay-ebpf/src/vmlinux.rs"
    ],
    "crates/assay-registry/src/rekor.rs": module.ALLOWED_MERKLE_USES[
        "crates/assay-registry/src/rekor.rs"
    ],
}
buf = io.StringIO()
with redirect_stdout(buf):
    rc = module.check_tree(root, chosen, identifiers={})
out = buf.getvalue()
if rc == 0 or "merkle_tree_probe" not in out:
    raise SystemExit("FAIL: vmlinux piggyback must fail:\n" + out)
print("ok: vmlinux-piggyback")
PY

# Plans remain scanned even when MkDocs excludes them from publication.
PLANS="$TMP/plans-publication-excluded"
init_fixture "$PLANS"
mkdir -p "$PLANS/docs/superpowers/plans"
cat > "$PLANS/mkdocs.yml" <<'YAML'
exclude_docs: |
  /superpowers/plans/
YAML
printf '%s\n' "$FALSE_INJECT" > "$PLANS/docs/superpowers/plans/sneak.md"
git -C "$PLANS" add -A -- mkdocs.yml docs/superpowers/plans/sneak.md
python3 - "$CHECKER" "$PLANS" <<'PY'
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
if getattr(module, "SCAN_PREFIX_EXCLUDES", None):
    raise SystemExit("FAIL: SCAN_PREFIX_EXCLUDES must not be a prefix escape hatch")
rekor = {
    "crates/assay-registry/src/rekor.rs": module.ALLOWED_MERKLE_USES[
        "crates/assay-registry/src/rekor.rs"
    ]
}
buf = io.StringIO()
with redirect_stdout(buf):
    rc = module.check_tree(root, rekor, identifiers={})
out = buf.getvalue()
if rc == 0 or "sneak.md" not in out:
    raise SystemExit("FAIL: publication-excluded plan with a false claim must fail:\n" + out)
print("ok: plans-scanned-despite-publication-exclude")
PY

# Corrected-history: original line without adjacent correction fails;
# changing the correction fails; sidecar-less frozen label fails.
HIST="$TMP/corrected-history"
init_fixture "$HIST"
mkdir -p "$HIST/docs/architecture"
printf '%s\n' \
  'This creates a lightweight **Hash Chain** (Merkle sequence) that proves the integrity and order of the event stream.' \
  > "$HIST/docs/architecture/ADR-007-Deterministic-Provenance.md"
git -C "$HIST" add -A -- docs/architecture/ADR-007-Deterministic-Provenance.md
python3 - "$CHECKER" "$HIST" <<'PY'
import importlib.util
import io
import subprocess
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
history = {
    "docs/architecture/ADR-007-Deterministic-Provenance.md": module.CORRECTED_HISTORY[
        "docs/architecture/ADR-007-Deterministic-Provenance.md"
    ]
}
# Monkeypatch production history for this fixture by writing the correction after.
buf = io.StringIO()
with redirect_stdout(buf):
    rc = module.check_tree(root, rekor, identifiers={})
out = buf.getvalue()
if rc == 0:
    raise SystemExit("FAIL: historical line without dated correction must fail:\n" + out)
adr = root / "docs/architecture/ADR-007-Deterministic-Provenance.md"
correction = "\n".join("> " + line for line in module.DATED_CORRECTION_BODY)
adr.write_text(adr.read_text() + "\n" + correction + "\n")
subprocess.run(
    ["git", "add", "-A", "--", "docs/architecture/ADR-007-Deterministic-Provenance.md"],
    cwd=root,
    check=True,
)
buf = io.StringIO()
with redirect_stdout(buf):
    rc = module.check_tree(root, rekor, identifiers={})
if rc != 0:
    raise SystemExit(
        "FAIL: historical line with adjacent dated correction must pass:\n" + buf.getvalue()
    )
mutated = adr.read_text().replace("newline-delimited", "space-delimited", 1)
adr.write_text(mutated)
subprocess.run(
    ["git", "add", "-A", "--", "docs/architecture/ADR-007-Deterministic-Provenance.md"],
    cwd=root,
    check=True,
)
buf = io.StringIO()
with redirect_stdout(buf):
    rc = module.check_tree(root, rekor, identifiers={})
out = buf.getvalue()
if rc == 0:
    raise SystemExit("FAIL: changed dated correction must fail:\n" + out)
print("ok: corrected-history-adjacent")
PY

# Membership claim without the word Merkle.
MEMBER="$TMP/membership"
init_fixture "$MEMBER"
_MEMBER_PREFIX='run_root lets an auditor prove one event was included'
_MEMBER_SUFFIX=' without reading the rest of the bundle.'
printf '%s\n' "${_MEMBER_PREFIX}${_MEMBER_SUFFIX}" >> "$MEMBER/docs/lint/index.md"
git -C "$MEMBER" add -A -- docs/lint/index.md
python3 - "$CHECKER" "$MEMBER" <<'PY'
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
if rc == 0 or "inclusion/membership" not in out:
    raise SystemExit("FAIL: run_root membership claim must fail even without Merkle:\n" + out)
print("ok: run-root-membership")
PY

# Order-aware negation: a later "not an inclusion" clause must not wash an
# earlier affirmative run_root membership claim.
ORDERNEG="$TMP/membership-order"
init_fixture "$ORDERNEG"
printf '%s\n' \
  'run_root gives inclusion proofs, although a flat digest is not an inclusion structure' \
  'run_root supports membership queries; note this is not a membership tree' \
  'The shipped run_root does not provide an inclusion proof.' \
  'run_root is not a membership structure.' \
  >> "$ORDERNEG/docs/lint/index.md"
git -C "$ORDERNEG" add -A -- docs/lint/index.md
python3 - "$CHECKER" "$ORDERNEG" <<'PY'
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
    raise SystemExit("FAIL: order-blind negation must not pass:\n" + out)
if "gives inclusion proofs" not in out:
    raise SystemExit("FAIL: first order-aware membership claim was not flagged:\n" + out)
if "supports membership queries" not in out:
    raise SystemExit("FAIL: second order-aware membership claim was not flagged:\n" + out)
if "does not provide an inclusion proof" in out:
    raise SystemExit("FAIL: genuine run_root negation was flagged:\n" + out)
if "is not a membership structure" in out:
    raise SystemExit("FAIL: genuine membership negation was flagged:\n" + out)
print("ok: membership-order-aware")
PY

# Mid-list dated correction is not a clean boundary; after the list item is.
MIDLIST="$TMP/mid-list-correction"
init_fixture "$MIDLIST"
mkdir -p "$MIDLIST/docs/architecture"
python3 - "$CHECKER" "$MIDLIST" <<'PY'
import importlib.util
import io
import subprocess
import sys
from contextlib import redirect_stdout
from pathlib import Path

checker, root = Path(sys.argv[1]), Path(sys.argv[2])
spec = importlib.util.spec_from_file_location("evidence_vocabulary", checker)
module = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(module)
item1_head = (
    "- Determinism is non-negotiable here: assay evidence is replayable (VCR) "
    "and Merkle-hashed. Redaction"
)
item1_tail = "  must be a pure, deterministic transform applied *before* hashing."
item2 = (
    "- Deterministic and replay-stable: same token, so\n"
    "  VCR replay and Merkle hashing stay stable."
)
item3 = (
    "- Belt-and-suspenders: a final ASSERTION sweep over the assembled ndjson "
    "before the Merkle root and\n"
    "  manifest are computed."
)
correction = "\n".join("> " + line for line in module.DATED_CORRECTION_BODY)
broken = (
    item1_head
    + "\n\n"
    + correction
    + "\n\n"
    + item1_tail
    + "\n"
    + item2
    + "\n"
    + item3
    + "\n"
)
adr = root / "docs/architecture/ADR-034-Evidence-Redaction-At-Capture.md"
adr.write_text(broken)
subprocess.run(
    ["git", "add", "-A", "--", "docs/architecture/ADR-034-Evidence-Redaction-At-Capture.md"],
    cwd=root,
    check=True,
)
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
    raise SystemExit("FAIL: mid-list dated correction must fail adjacency:\n" + out)
clean = (
    item1_head
    + "\n"
    + item1_tail
    + "\n\n"
    + correction
    + "\n"
    + item2
    + "\n\n"
    + correction
    + "\n"
    + item3
    + "\n\n"
    + correction
    + "\n"
)
adr.write_text(clean)
subprocess.run(
    ["git", "add", "-A", "--", "docs/architecture/ADR-034-Evidence-Redaction-At-Capture.md"],
    cwd=root,
    check=True,
)
lines = clean.splitlines()
idx = next(i for i, line in enumerate(lines) if "Merkle-hashed" in line)
start, end = module.enclosing_block_span(lines, idx, adr.name)
if module.blockquote_inside_span(lines, start, end):
    raise SystemExit("FAIL: clean list item still contains a mid-item blockquote")
if start + 1 >= end:
    raise SystemExit("FAIL: enclosing list item must include the continuation line")
buf = io.StringIO()
with redirect_stdout(buf):
    rc = module.check_tree(root, rekor, identifiers={})
if rc != 0:
    raise SystemExit(
        "FAIL: correction after the full list item must pass:\n" + buf.getvalue()
    )
print("ok: correction-list-boundary")
PY

# Self-test only: repo-pinned pymdown-extensions (MkDocs fence engine).
# The live guard must not import markdown.
PYMD_PIN="$(
  python3 - "$ROOT/docs/requirements-ci.txt" <<'PY'
from pathlib import Path
import sys
for line in Path(sys.argv[1]).read_text().splitlines():
    if line.startswith("pymdown-extensions=="):
        print(line.strip())
        raise SystemExit(0)
raise SystemExit("FAIL: docs/requirements-ci.txt must pin pymdown-extensions")
PY
)"
RENDER_VENV="$TMP/render-venv"
RENDER_PY="$RENDER_VENV/bin/python"
if ! python3 -c "import markdown, pymdownx.superfences" 2>/dev/null; then
  python3 -m venv "$RENDER_VENV"
  "$RENDER_VENV/bin/pip" install -q "$PYMD_PIN"
else
  RENDER_PY="python3"
fi

# Ordered-list continuity via real Python-Markdown / superfences, plus the
# production adjacency rule (no second renderer in the live guard).
OLSPLIT="$TMP/ol-split"
init_fixture "$OLSPLIT"
mkdir -p "$OLSPLIT/docs/architecture"
"$RENDER_PY" - "$CHECKER" "$OLSPLIT" "$ROOT" <<'PY'
import importlib.util
import io
import subprocess
import sys
from contextlib import redirect_stdout
from html.parser import HTMLParser
from pathlib import Path

import markdown

checker, root, repo = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
spec = importlib.util.spec_from_file_location("evidence_vocabulary", checker)
module = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(module)
src = (repo / "scripts/ci/check-evidence-vocabulary.py").read_text()
if "_block_events" in src or "_ordered_list_runs" in src:
    raise SystemExit("FAIL: live guard must not ship a second Markdown renderer")
if "import markdown" in src or "pymdownx" in src:
    raise SystemExit("FAIL: live guard must not import the MkDocs markdown stack")
mkdocs = (repo / "mkdocs.yml").read_text()
if "pymdownx.superfences" not in mkdocs:
    raise SystemExit("FAIL: repo mkdocs.yml must name pymdownx.superfences")


class _TopOl(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.ols: list[str] = []
        self._depth = 0
        self._parts: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        if tag == "ol":
            if self._depth == 0:
                self._parts = []
            self._depth += 1

    def handle_endtag(self, tag: str) -> None:
        if tag == "ol" and self._depth:
            self._depth -= 1
            if self._depth == 0:
                self.ols.append("".join(self._parts))

    def handle_data(self, data: str) -> None:
        if self._depth:
            self._parts.append(data)


def render_ols(text: str) -> list[str]:
    html = markdown.markdown(text, extensions=["pymdownx.superfences"])
    parser = _TopOl()
    parser.feed(html)
    return parser.ols


def one_ol_contains(text: str, needles: tuple[str, ...]) -> bool:
    return any(all(needle in blob for needle in needles) for blob in render_ols(text))


correction = "\n".join("> " + line for line in module.DATED_CORRECTION_BODY)
needles = ("Wilson-lower-bound gating", "Security hardening")
split_src = (
    "1. Wilson-lower-bound gating\n"
    "2. Content-addressed replay\n"
    "3. Typed VCR\n"
    "4. Evidence integrity chain - Merkle root\n"
    "\n"
    + correction
    + "\n\n"
    "5. Adaptive judge\n"
    "6. Security hardening\n"
)
one_ol = (
    "1. Wilson-lower-bound gating\n"
    "2. Content-addressed replay\n"
    "3. Typed VCR\n"
    "4. Evidence integrity chain - Merkle root\n"
    "5. Adaptive judge\n"
    "6. Security hardening\n"
    "\n"
    + correction
    + "\n"
)
fenced = (
    "```md\n"
    "1. Wilson-lower-bound gating\n"
    "2. Content-addressed replay\n"
    "\n"
    + correction
    + "\n\n"
    "5. Adaptive judge\n"
    "6. Security hardening\n"
    "```\n"
    "\n"
    "1. Wilson-lower-bound gating\n"
    "2. Content-addressed replay\n"
    "3. Typed VCR\n"
    "4. Evidence integrity chain - Merkle root\n"
    "5. Adaptive judge\n"
    "6. Security hardening\n"
)
if len(render_ols(split_src)) != 2 or one_ol_contains(split_src, needles):
    raise SystemExit(
        f"FAIL: mid-list correction must split <ol> under Python-Markdown: {render_ols(split_src)!r}"
    )
if len(render_ols(one_ol)) != 1 or not one_ol_contains(one_ol, needles):
    raise SystemExit(
        f"FAIL: after-list correction must keep one <ol>: {render_ols(one_ol)!r}"
    )
if len(render_ols(fenced)) != 1 or not one_ol_contains(fenced, needles):
    raise SystemExit(
        "FAIL: fenced split-list lookalike must not be counted as a document <ol>"
    )
rfc = root / "docs/architecture/RFC-001-dx-ux-governance.md"
rfc.write_text(split_src)
subprocess.run(
    ["git", "add", "-A", "--", "docs/architecture/RFC-001-dx-ux-governance.md"],
    cwd=root,
    check=True,
)
rekor = {
    "crates/assay-registry/src/rekor.rs": module.ALLOWED_MERKLE_USES[
        "crates/assay-registry/src/rekor.rs"
    ]
}
module.CORRECTED_HISTORY = {
    "docs/architecture/RFC-001-dx-ux-governance.md": (
        module.re.escape("4. Evidence integrity chain - Merkle root"),
    )
}
buf = io.StringIO()
with redirect_stdout(buf):
    rc = module.check_tree(root, rekor, identifiers={})
out = buf.getvalue()
if rc == 0:
    raise SystemExit("FAIL: mid-<ol> dated correction must fail adjacency:\n" + out)
rfc.write_text(one_ol)
subprocess.run(
    ["git", "add", "-A", "--", "docs/architecture/RFC-001-dx-ux-governance.md"],
    cwd=root,
    check=True,
)
buf = io.StringIO()
with redirect_stdout(buf):
    rc = module.check_tree(root, rekor, identifiers={})
if rc != 0:
    raise SystemExit(
        "FAIL: correction after the full ordered list must pass:\n" + buf.getvalue()
    )
print("ok: correction-ordered-list-render")
PY

# Dated correction must sit below the historical wording, matching "above".
DIRCORR="$TMP/correction-direction"
init_fixture "$DIRCORR"
mkdir -p "$DIRCORR/docs/architecture"
python3 - "$CHECKER" "$DIRCORR" <<'PY'
import importlib.util
import io
import subprocess
import sys
from contextlib import redirect_stdout
from pathlib import Path

checker, root = Path(sys.argv[1]), Path(sys.argv[2])
spec = importlib.util.spec_from_file_location("evidence_vocabulary", checker)
module = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(module)
correction = "\n".join("> " + line for line in module.DATED_CORRECTION_BODY)
hist = (
    "This creates a lightweight **Hash Chain** (Merkle sequence) that proves "
    "the integrity and order of the event stream."
)
adr = root / "docs/architecture/ADR-007-Deterministic-Provenance.md"
adr.write_text(correction + "\n\n" + hist + "\n")
subprocess.run(
    ["git", "add", "-A", "--", "docs/architecture/ADR-007-Deterministic-Provenance.md"],
    cwd=root,
    check=True,
)
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
        "FAIL: dated correction above the historical wording must fail:\n" + out
    )
print("ok: correction-directional-above")
PY

# SPEC §7 must carry the boundary formula; delimiter-free must fail.
FORMULA="$TMP/formula-parity"
init_fixture "$FORMULA"
mkdir -p "$FORMULA/docs/architecture" "$FORMULA/crates/assay-cli/src/exit_codes"
python3 - "$CHECKER" "$FORMULA" <<'PY'
import importlib.util
import io
import subprocess
import sys
from contextlib import redirect_stdout
from pathlib import Path

checker, root = Path(sys.argv[1]), Path(sys.argv[2])
spec = importlib.util.spec_from_file_location("evidence_vocabulary", checker)
module = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(module)
boundary_src = module.REPO_ROOT / module.CANONICAL_FORMULA_SOURCE
spec_src = module.REPO_ROOT / module.SPEC_OUTWARD_REL
boundary = root / module.CANONICAL_FORMULA_SOURCE
spec_path = root / module.SPEC_OUTWARD_REL
boundary.parent.mkdir(parents=True, exist_ok=True)
spec_path.parent.mkdir(parents=True, exist_ok=True)
boundary.write_text(boundary_src.read_text())
spec_path.write_text(spec_src.read_text())
subprocess.run(
    ["git", "add", "-A", "--", module.CANONICAL_FORMULA_SOURCE, module.SPEC_OUTWARD_REL],
    cwd=root,
    check=True,
)
formula = module.canonical_run_root_formula(root)
if "newline-delimited" not in formula or "trailing newline" not in formula:
    raise SystemExit(f"FAIL: extracted formula lost required tokens: {formula!r}")
section = module.spec_section_text(spec_path.read_text(), "7.")
if formula not in module.flow_whitespace(section):
    raise SystemExit("FAIL: fixture SPEC §7 must contain the extracted formula verbatim")
control = module.formula_parity_findings(root)
if control:
    raise SystemExit("FAIL: formula-parity control must pass:\n" + "\n".join(control))
mutated = spec_path.read_text().replace("newline-delimited", "delimiter-free concatenated", 1)
mutated = mutated.replace("trailing newline", "no trailing newline", 1)
if "delimiter-free" not in mutated or "no trailing newline" not in mutated:
    raise SystemExit("FAIL: mutation did not land in SPEC §7")
spec_path.write_text(mutated)
subprocess.run(
    ["git", "add", "-A", "--", module.SPEC_OUTWARD_REL],
    cwd=root,
    check=True,
)
findings = module.formula_parity_findings(root)
if not findings:
    raise SystemExit("FAIL: delimiter-free / no trailing newline in SPEC §7 must fail")
print("ok: formula-parity-spec-section-7")
PY

# run_root + hash/integrity chain is a current-claim, not general chain prose.
CHAIN="$TMP/run-root-chain"
init_fixture "$CHAIN"
printf '%s\n' \
  'manifest.run_root, a chain over per-event content hashes' \
  'the integrity chain does what the profile says' \
  'The shipped run_root is not a hash chain.' \
  'run_root is not an integrity chain.' \
  >> "$CHAIN/docs/lint/index.md"
git -C "$CHAIN" add -A -- docs/lint/index.md
python3 - "$CHECKER" "$CHAIN" <<'PY'
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
if not module.is_run_root_chain_claim(
    "manifest.run_root, a chain over per-event content hashes"
):
    raise SystemExit("FAIL: run_root + chain over must be a chain claim")
if module.is_run_root_chain_claim("the integrity chain does what the profile says"):
    raise SystemExit("FAIL: general integrity-chain prose must not be banned")
if module.is_run_root_chain_claim("The shipped run_root is not a hash chain."):
    raise SystemExit("FAIL: negated hash-chain sentence must pass")
if module.is_run_root_chain_claim("run_root is not an integrity chain."):
    raise SystemExit("FAIL: negated integrity-chain sentence must pass")
rekor = {
    "crates/assay-registry/src/rekor.rs": module.ALLOWED_MERKLE_USES[
        "crates/assay-registry/src/rekor.rs"
    ]
}
buf = io.StringIO()
with redirect_stdout(buf):
    rc = module.check_tree(root, rekor, identifiers={})
out = buf.getvalue()
if rc == 0 or "chain over per-event" not in out:
    raise SystemExit("FAIL: run_root chain claim must fail:\n" + out)
if "the integrity chain does what the profile says" in out:
    raise SystemExit("FAIL: general chain language was banned:\n" + out)
if "is not a hash chain" in out or "is not an integrity chain" in out:
    raise SystemExit("FAIL: negated run_root chain sentence was flagged:\n" + out)
print("ok: run-root-chain-claim")
PY

# tree-root variant plus exact-line allowlist must not mask it.
reset_lint
printf '%s\n' 'tree root' 'run_root is a tree root' >> "$FIXTURE/docs/lint/index.md"
git -C "$FIXTURE" add -A -- docs/lint/index.md
python3 - "$CHECKER" "$FIXTURE" <<'PY'
import importlib.util
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
line = "run_root is a tree root"
if not module.FALSE_CLAIM_RE.search(line):
    raise SystemExit("FAIL: FALSE_CLAIM_RE must match the tree-root variant")
rekor = {
    "crates/assay-registry/src/rekor.rs": module.ALLOWED_MERKLE_USES[
        "crates/assay-registry/src/rekor.rs"
    ]
}
chosen = {**rekor, "docs/lint/index.md": (re.escape("tree root"),)}
buf = io.StringIO()
with redirect_stdout(buf):
    rc = module.check_tree(root, chosen, identifiers={})
out = buf.getvalue()
if rc == 0 or "run_root is a tree root" not in out:
    raise SystemExit(
        "FAIL: exact-line allowlist must not mask run_root is a tree root:\n" + out
    )
print("ok: tree-root-allowlist-mask")
PY

# Permit as a strict substring of a longer Merkle line, without run_root.
SUBSTR="$TMP/substring-permit"
init_fixture "$SUBSTR"
printf '%s\n' 'The evidence digest is a Merkle tree over every recorded event.' \
  >> "$SUBSTR/docs/lint/index.md"
git -C "$SUBSTR" add -A -- docs/lint/index.md
python3 - "$CHECKER" "$SUBSTR" <<'PY'
import importlib.util
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
line = "The evidence digest is a Merkle tree over every recorded event."
pat = re.compile(re.escape("Merkle tree"), re.IGNORECASE)
if pat.search(line) is None:
    raise SystemExit("FAIL: fixture line must contain the substring permit")
if module.line_matches(line, pat):
    raise SystemExit("FAIL: substring permit fullmatched a longer false-claim line")
chosen = {
    "docs/lint/index.md": (re.escape("Merkle tree"),),
    "crates/assay-registry/src/rekor.rs": module.ALLOWED_MERKLE_USES[
        "crates/assay-registry/src/rekor.rs"
    ],
}
buf = io.StringIO()
with redirect_stdout(buf):
    rc = module.check_tree(root, chosen, identifiers={})
out = buf.getvalue()
if rc == 0 or "Merkle tree over every recorded event" not in out:
    raise SystemExit("FAIL: substring permit must not admit a longer line:\n" + out)
print("ok: substring-permit-fullmatch")
PY

# Renamed E3 harness sibling cannot escape the withdrawn-label scan.
E3SIB="$TMP/e3-sibling"
init_fixture "$E3SIB"
mkdir -p "$E3SIB/crates/assay-evidence/tests"
printf '%s\n' 'fn inclusion_proof_hashes(n: u64) -> u32 { n as u32 }' \
  > "$E3SIB/crates/assay-evidence/tests/e3_verify_cost_curve_v2.rs"
git -C "$E3SIB" add -A -- crates/assay-evidence/tests/e3_verify_cost_curve_v2.rs
python3 - "$CHECKER" "$E3SIB" <<'PY'
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
if rc == 0 or "e3_verify_cost_curve_v2.rs" not in out:
    raise SystemExit("FAIL: renamed E3 harness sibling escaped withdrawn scan:\n" + out)
if not module.is_withdrawn_surface(
    "crates/assay-evidence/tests/e3_verify_cost_curve_v2.rs"
):
    raise SystemExit("FAIL: is_withdrawn_surface must cover the E3 harness family")
if module.is_withdrawn_surface("crates/assay-evidence/tests/writer_verifier_symmetry.rs"):
    raise SystemExit("FAIL: non-E3 evidence tests must not be withdrawn surfaces")
print("ok: withdrawn-e3-family")
PY

# Aggregator must render frozen 2026-06 cost.json keys.
python3 - "$ROOT/docs/experiments/evidence-mutation-cost-2026-06/aggregate.py" \
  "$ROOT/docs/experiments/evidence-mutation-cost-2026-06/results/cost.json" <<'PY'
import importlib.util
import json
import sys
from pathlib import Path

agg_path, cost_path = Path(sys.argv[1]), Path(sys.argv[2])
spec = importlib.util.spec_from_file_location("e3_aggregate", agg_path)
module = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(module)
cost = json.loads(cost_path.read_text())
if "synthetic_log2_hash_count" in json.dumps(cost):
    raise SystemExit("FAIL: frozen cost.json must keep the historical column name")
rendered = module.render_cost(cost)
if "inclusion-proof hashes" not in rendered:
    raise SystemExit("FAIL: aggregator did not render the frozen column header")
if "| 1,000 |" not in rendered or "| 10 |" not in rendered:
    raise SystemExit("FAIL: aggregator dropped frozen measurement rows:\n" + rendered)
print("ok: frozen-cost-aggregate")
PY

# A new dated experiment directory still carries withdrawn labels.
NEWEXP="$TMP/withdrawn-2026-09"
init_fixture "$NEWEXP"
mkdir -p "$NEWEXP/docs/experiments/evidence-mutation-cost-2026-09/results"
printf '%s\n' 'inclusion_proof_hashes: 4' \
  > "$NEWEXP/docs/experiments/evidence-mutation-cost-2026-09/results/cost.json"
git -C "$NEWEXP" add -A -- \
  docs/experiments/evidence-mutation-cost-2026-09/results/cost.json
python3 - "$CHECKER" "$NEWEXP" <<'PY'
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
if rc == 0 or "2026-09" not in out:
    raise SystemExit("FAIL: withdrawn-2026-09 expected fail:\n" + out)
if not module.is_withdrawn_surface(
    "docs/experiments/evidence-mutation-cost-2026-09/results/cost.json"
):
    raise SystemExit("FAIL: is_withdrawn_surface must cover docs/experiments/ generally")
print("ok: withdrawn-2026-09")
PY

"$RENDER_PY" - "$CHECKER" "$ROOT" <<'PY'
import importlib.util
import inspect
import io
import re
import sys
from contextlib import redirect_stdout
from html.parser import HTMLParser
from pathlib import Path

import markdown

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
if getattr(module, "SCAN_PREFIX_EXCLUDES", None):
    raise SystemExit("FAIL: SCAN_PREFIX_EXCLUDES must not exist as a prefix escape hatch")
if module.is_excluded("docs/superpowers/plans/example.md"):
    raise SystemExit("FAIL: publication status must not exclude repository-visible plans")
if not module.is_excluded("scripts/ci/check-evidence-vocabulary.py"):
    raise SystemExit("FAIL: checker self-text must remain an exact path exclusion")
history = getattr(module, "CORRECTED_HISTORY", None)
if not isinstance(history, dict) or not history:
    raise SystemExit("FAIL: CORRECTED_HISTORY must be a non-empty path-bound dict")
if set(history) & set(allow):
    raise SystemExit("FAIL: CORRECTED_HISTORY must not mix with ALLOWED_MERKLE_USES")
if "docs/experiments/" != getattr(module, "WITHDRAWN_EXPERIMENT_PREFIX", None):
    raise SystemExit("FAIL: withdrawn labels must cover docs/experiments/ generally")
if not callable(getattr(module, "is_run_root_membership_claim", None)):
    raise SystemExit("FAIL: is_run_root_membership_claim must be the shared membership rule")
hostile = getattr(module, "HOSTILE_GIT_ENV_NAMES", ())
script = (root / "scripts/ci/lib/clear-git-repository-env.sh").read_text()
script_names = {
    token
    for line in script.splitlines()
    if not line.startswith("#")
    for token in line.replace("\\", " ").split()
    if token.startswith("GIT_")
}
if set(hostile) != script_names:
    raise SystemExit(
        f"FAIL: HOSTILE_GIT_ENV_NAMES must match clear-git-repository-env.sh: {set(hostile)!r} vs {script_names!r}"
    )
ci_text = (root / ".github/workflows/ci.yml").read_text()
scope = __import__("re").search(r"(?ms)^  scope:\n(.*?)(?=^  [a-zA-Z][\w-]*:|\Z)", ci_text)
if not scope:
    raise SystemExit("FAIL: ci.yml scope job missing")
if "bash scripts/ci/test-evidence-vocabulary.sh" not in scope.group(1):
    raise SystemExit("FAIL: ci.yml scope job must invoke the evidence-vocabulary self-test")
if "python3 scripts/ci/check-evidence-vocabulary.py" not in scope.group(1):
    raise SystemExit("FAIL: ci.yml scope job must invoke the live evidence-vocabulary checker")
for rel in allow:
    if rel in guard_paths:
        raise SystemExit(f"FAIL: guard path {rel} must not be in ALLOWED_MERKLE_USES")
    if "verify_side_effects.rs" in rel:
        raise SystemExit("FAIL: verify_side_effects.rs must not have an allowlist exception")
    for pat in allow[rel]:
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

if "line_matches" not in inspect.getsource(module.line_is_allowed):
    raise SystemExit("FAIL: line_is_allowed must call line_matches")
if "line_matches" not in inspect.getsource(module.allowlist_staleness):
    raise SystemExit("FAIL: staleness must use the same line_matches rule")
if getattr(module, "WITHDRAWN_HARNESS", None) is not None:
    raise SystemExit("FAIL: WITHDRAWN_HARNESS exact filename must not return; use the E3 family")
if getattr(module, "RUN_ROOT_MEMBERSHIP_NEGATION_RE", None) is not None:
    raise SystemExit("FAIL: line-global membership negation regex must not return")
if "below" in " ".join(module.DATED_CORRECTION_BODY).lower():
    raise SystemExit("FAIL: dated correction must not say the historical wording is below")
for rel in (
    "docs/architecture/ADR-034-Evidence-Redaction-At-Capture.md",
    "docs/architecture/ADR-039-evidence-bundle-attestation.md",
    "docs/architecture/RFC-001-dx-ux-governance.md",
    "docs/experiments/evidence-mutation-cost-2026-06/README.md",
):
    lines = (root / rel).read_text().splitlines()
    for idx, line in enumerate(lines):
        if not module.line_is_corrected_history(rel, line):
            continue
        start, end = module.enclosing_structure_span(lines, idx, rel)
        if module.blockquote_inside_span(lines, start, end):
            raise SystemExit(
                f"FAIL: {rel} still places a dated correction inside a list or paragraph"
            )
        if not module.has_adjacent_dated_correction(lines, idx, rel):
            raise SystemExit(
                f"FAIL: {rel} historical line is not followed by a dated correction"
            )

class _TopOl(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.ols: list[str] = []
        self._depth = 0
        self._parts: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        if tag == "ol":
            if self._depth == 0:
                self._parts = []
            self._depth += 1

    def handle_endtag(self, tag: str) -> None:
        if tag == "ol" and self._depth:
            self._depth -= 1
            if self._depth == 0:
                self.ols.append("".join(self._parts))

    def handle_data(self, data: str) -> None:
        if self._depth:
            self._parts.append(data)

def _one_ol_contains(text: str, needles: tuple[str, ...]) -> bool:
    html = markdown.markdown(text, extensions=["pymdownx.superfences"])
    parser = _TopOl()
    parser.feed(html)
    return any(all(needle in blob for needle in needles) for blob in parser.ols)

for rel, needles in (
    (
        "docs/architecture/RFC-001-dx-ux-governance.md",
        ("Wilson-lower-bound gating", "Security hardening"),
    ),
    (
        "docs/experiments/evidence-mutation-cost-2026-06/README.md",
        ("mutation-detection matrix", "verification + signing cost curve"),
    ),
):
    if not _one_ol_contains((root / rel).read_text(), needles):
        raise SystemExit(
            f"FAIL: {rel} does not render as one continuous <ol> under "
            "Python-Markdown / pymdownx.superfences"
        )

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
