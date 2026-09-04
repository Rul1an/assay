#!/usr/bin/env bash
# Behavioral mutations for scripts/ci/publish_idempotent.sh --check-order.
# No Cargo, no crates.io network, no publish. Fake cargo proves check mode cannot publish.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$ROOT/scripts/ci/publish_idempotent.sh"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

mutation_count=0
EXPECTED_CASES=10

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

pass_case() {
  local name="$1"
  mutation_count=$((mutation_count + 1))
  echo "ok: $name"
}

write_fake_cargo() {
  local path="$1"
  local marker="$2"
  cat >"$path" <<EOF
#!/usr/bin/env bash
printf '%s\n' "\$*" >>"$marker"
if [[ "\${1:-}" == "publish" ]]; then
  echo "fake-cargo: refused publish" >&2
  exit 97
fi
exit 0
EOF
  chmod +x "$path"
}

sparse_prefix() {
  local crate="$1"
  local n=${#crate}
  if [[ "$n" -eq 1 ]]; then
    printf '1'
  elif [[ "$n" -eq 2 ]]; then
    printf '2'
  elif [[ "$n" -eq 3 ]]; then
    printf '3/%s' "${crate:0:1}"
  else
    printf '%s/%s' "${crate:0:2}" "${crate:2:2}"
  fi
}

write_index_crate() {
  local index_root="$1"
  local crate="$2"
  local vers="$3"
  local prefix
  prefix="$(sparse_prefix "$crate")"
  mkdir -p "$index_root/$prefix"
  printf '%s\n' "{\"name\":\"$crate\",\"vers\":\"$vers\",\"deps\":[],\"cksum\":\"0\",\"features\":{},\"yanked\":false}" \
    >"$index_root/$prefix/$crate"
}

prepare_repo_scratch() {
  local dest="$1"
  rm -rf "$dest"
  mkdir -p "$dest/scripts/ci" "$dest/crates"
  cp -R "$ROOT/crates/." "$dest/crates/"
  cp "$ROOT/Cargo.toml" "$dest/Cargo.toml"
  cp "$SCRIPT" "$dest/scripts/ci/publish_idempotent.sh"
  chmod +x "$dest/scripts/ci/publish_idempotent.sh"
}

run_check() {
  local repo="$1"
  shift
  (
    cd "$repo"
    env "$@" ./scripts/ci/publish_idempotent.sh --check-order
  )
}

omit_crate_from_crates_array() {
  local script_path="$1"
  local crate="$2"
  python3 - "$script_path" "$crate" <<'PY'
import pathlib, re, sys
path = pathlib.Path(sys.argv[1])
crate = sys.argv[2]
text = path.read_text(encoding="utf-8")
pattern = rf'(?m)^[ \t]*"{re.escape(crate)}"\n'
new, n = re.subn(pattern, "", text, count=1)
if n != 1:
    raise SystemExit(f"failed to omit {crate}: matches={n}")
path.write_text(new, encoding="utf-8")
PY
}

swap_crates_in_array() {
  local script_path="$1"
  local a="$2"
  local b="$3"
  python3 - "$script_path" "$a" "$b" <<'PY'
import pathlib, sys
path = pathlib.Path(sys.argv[1])
a, b = sys.argv[2], sys.argv[3]
text = path.read_text(encoding="utf-8")
qa, qb = f'"{a}"', f'"{b}"'
if text.count(qa) != 1 or text.count(qb) != 1:
    raise SystemExit(f"expected one occurrence each of {a!r} and {b!r}")
text = text.replace(qa, "__SWAP_A__").replace(qb, qa).replace("__SWAP_A__", qb)
path.write_text(text, encoding="utf-8")
PY
}

[[ -f "$SCRIPT" ]] || fail "publish_idempotent.sh missing"
[[ -x "$SCRIPT" ]] || chmod +x "$SCRIPT"

# Fail fast before any invocation that could fall through into the publish loop
# (15 crates × sleep 10). Missing --check-order is itself a required RED.
if ! grep -q -- '--check-order' "$SCRIPT"; then
  fail "production script must implement --check-order"
fi
if ! grep -qE '^validate_publish_order$' "$SCRIPT"; then
  fail "production publish_idempotent.sh must call validate_publish_order"
fi

FAKEBIN="$SCRATCH/fakebin"
MARKER="$SCRATCH/cargo-publish.marker"
mkdir -p "$FAKEBIN"
: >"$MARKER"
write_fake_cargo "$FAKEBIN/cargo" "$MARKER"

INDEX="$SCRATCH/index"
write_index_crate "$INDEX" "assay-runner-core" "6.0.0"
INDEX_URI="file://${INDEX}"

# --- unknown argument fails closed before publish ---
unknown_rc=0
unknown_out="$(
  PATH="$FAKEBIN:$PATH" "$SCRIPT" --not-a-real-flag 2>&1
)" || unknown_rc=$?
if [[ "$unknown_rc" -ne 2 ]]; then
  fail "unknown arg expected exit 2, got $unknown_rc; out=$unknown_out"
fi
if [[ -s "$MARKER" ]]; then
  fail "unknown arg must not invoke cargo; marker=$(cat "$MARKER")"
fi
pass_case "unknown-arg-exit-2"

# --- --check-order exists and does not publish ---
: >"$MARKER"
check_rc=0
check_out="$(
  cd "$ROOT"
  PATH="$FAKEBIN:$PATH" ASSAY_CRATES_INDEX_BASE="$INDEX_URI" \
    "$SCRIPT" --check-order 2>&1
)" || check_rc=$?
if [[ "$check_rc" -ne 0 ]]; then
  fail "baseline --check-order expected 0, got $check_rc; out=$check_out"
fi
if [[ -s "$MARKER" ]]; then
  fail "baseline --check-order must not invoke cargo; marker=$(cat "$MARKER")"
fi
if ! grep -q 'publish order respects the dependency graph (15 crates)' <<<"$check_out"; then
  fail "baseline missing success line; out=$check_out"
fi
pass_case "baseline-check-order-no-publish"

# --- dependency after consumer ---
repo="$SCRATCH/swap-order"
prepare_repo_scratch "$repo"
swap_crates_in_array "$repo/scripts/ci/publish_idempotent.sh" "assay-common" "assay-registry"
swap_rc=0
swap_out="$(
  PATH="$FAKEBIN:$PATH" ASSAY_CRATES_INDEX_BASE="$INDEX_URI" \
    run_check "$repo" 2>&1
)" || swap_rc=$?
if [[ "$swap_rc" -eq 0 ]]; then
  fail "swapped common/registry must fail; out=$swap_out"
fi
if ! grep -qiE 'depends on assay-common|publish order does not respect' <<<"$swap_out"; then
  fail "swap missing order diagnostic; out=$swap_out"
fi
pass_case "dep-after-consumer"

# --- workspace=true silent skip bite: omit listed dep + absent major ---
# Today requirement_for returns None for workspace=true and clause 3 continues green.
# After the fix, workspace resolution must consult the index and fail on major mismatch.
repo="$SCRATCH/omit-absent-major"
prepare_repo_scratch "$repo"
omit_crate_from_crates_array "$repo/scripts/ci/publish_idempotent.sh" "assay-runner-core"
rm -rf "$SCRATCH/index-absent"
mkdir -p "$SCRATCH/index-absent"
write_index_crate "$SCRATCH/index-absent" "assay-runner-core" "5.5.2"
omit_rc=0
omit_out="$(
  PATH="$FAKEBIN:$PATH" ASSAY_CRATES_INDEX_BASE="file://${SCRATCH}/index-absent" \
    run_check "$repo" 2>&1
)" || omit_rc=$?
if [[ "$omit_rc" -eq 0 ]]; then
  fail "omitting assay-runner-core with absent major must fail (no workspace silent-skip); out=$omit_out"
fi
if ! grep -qiE 'does not have|newest published|assay-runner-core' <<<"$omit_out"; then
  fail "omit-absent-major missing diagnostic; out=$omit_out"
fi
pass_case "workspace-true-omit-absent-major"

# --- unlisted dep with matching major: clause 3 green ---
repo="$SCRATCH/omit-present-major"
prepare_repo_scratch "$repo"
omit_crate_from_crates_array "$repo/scripts/ci/publish_idempotent.sh" "assay-runner-core"
present_rc=0
present_out="$(
  PATH="$FAKEBIN:$PATH" ASSAY_CRATES_INDEX_BASE="$INDEX_URI" \
    run_check "$repo" 2>&1
)" || present_rc=$?
if [[ "$present_rc" -ne 0 ]]; then
  fail "present major for omitted unlisted dep should pass clause 3; out=$present_out"
fi
pass_case "unlisted-present-major"

# --- unreachable index ---
repo="$SCRATCH/bad-index"
prepare_repo_scratch "$repo"
omit_crate_from_crates_array "$repo/scripts/ci/publish_idempotent.sh" "assay-runner-core"
bad_rc=0
bad_out="$(
  PATH="$FAKEBIN:$PATH" ASSAY_CRATES_INDEX_BASE="file://${SCRATCH}/no-such-index-dir" \
    run_check "$repo" 2>&1
)" || bad_rc=$?
if [[ "$bad_rc" -eq 0 ]]; then
  fail "unreachable index must fail; out=$bad_out"
fi
if ! grep -qiE 'could not be reached|could-not-check|No such file|not a pass' <<<"$bad_out"; then
  fail "unreachable index missing diagnostic; out=$bad_out"
fi
pass_case "unreachable-index"

# --- unresolved workspace requirement ---
repo="$SCRATCH/unresolved-req"
prepare_repo_scratch "$repo"
omit_crate_from_crates_array "$repo/scripts/ci/publish_idempotent.sh" "assay-runner-core"
python3 - "$repo/Cargo.toml" <<'PY'
from pathlib import Path
import re, sys
path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
new, n = re.subn(
    r'(?m)^assay-runner-core\s*=\s*\{[^\}]*\}\n',
    "",
    text,
    count=1,
)
if n != 1:
    raise SystemExit(f"workspace.dependencies assay-runner-core matches={n}")
path.write_text(new, encoding="utf-8")
PY
unres_rc=0
unres_out="$(
  PATH="$FAKEBIN:$PATH" ASSAY_CRATES_INDEX_BASE="$INDEX_URI" \
    run_check "$repo" 2>&1
)" || unres_rc=$?
if [[ "$unres_rc" -eq 0 ]]; then
  fail "unresolved workspace requirement must fail; out=$unres_out"
fi
if ! grep -qiE 'no version requirement could be resolved|could not be resolved' <<<"$unres_out"; then
  fail "unresolved requirement missing diagnostic; out=$unres_out"
fi
pass_case "unresolved-workspace-requirement"

# --- listed publish=false ---
repo="$SCRATCH/unpublishable"
prepare_repo_scratch "$repo"
python3 - "$repo/crates/assay-common/Cargo.toml" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
lines = text.splitlines(True)
out = []
inserted = False
for line in lines:
    out.append(line)
    if not inserted and line.strip() == "[package]":
        out.append("publish = false\n")
        inserted = True
if not inserted:
    raise SystemExit("no [package]")
path.write_text("".join(out), encoding="utf-8")
PY
pub_rc=0
pub_out="$(
  PATH="$FAKEBIN:$PATH" ASSAY_CRATES_INDEX_BASE="$INDEX_URI" \
    run_check "$repo" 2>&1
)" || pub_rc=$?
if [[ "$pub_rc" -eq 0 ]]; then
  fail "publish=false listed crate must fail; out=$pub_out"
fi
if ! grep -qi 'publish = false' <<<"$pub_out"; then
  fail "publish=false missing diagnostic; out=$pub_out"
fi
pass_case "listed-publish-false"

# --- missing crate directory ---
repo="$SCRATCH/missing-crate"
prepare_repo_scratch "$repo"
rm -rf "$repo/crates/assay-policy"
miss_rc=0
miss_out="$(
  PATH="$FAKEBIN:$PATH" ASSAY_CRATES_INDEX_BASE="$INDEX_URI" \
    run_check "$repo" 2>&1
)" || miss_rc=$?
if [[ "$miss_rc" -eq 0 ]]; then
  fail "missing crate dir must fail; out=$miss_out"
fi
if ! grep -qi 'no crates/assay-policy/Cargo.toml' <<<"$miss_out"; then
  fail "missing crate missing diagnostic; out=$miss_out"
fi
pass_case "missing-crate-manifest"

# --- production wiring ---
if ! grep -qE '^validate_publish_order$' "$SCRIPT"; then
  fail "production publish_idempotent.sh must call validate_publish_order"
fi
if ! grep -q -- '--check-order' "$SCRIPT"; then
  fail "production script must implement --check-order"
fi
pass_case "production-validator-wired"

if [[ "$mutation_count" -ne "$EXPECTED_CASES" ]]; then
  fail "mutation_count=$mutation_count expected=$EXPECTED_CASES"
fi

echo "publish_idempotent --check-order mutations OK ($mutation_count)"
