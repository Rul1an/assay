#!/usr/bin/env bash
# Exercise the unchanged Linux gate with a tiny offline Cargo workspace and the real compiler.
# Mutants live outside the source tree. Normal exits remove that owned scratch; SIGKILL cannot.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=scripts/ci/lib/clear-git-repository-env.sh
source "$ROOT/scripts/ci/lib/clear-git-repository-env.sh"

# The line-oriented shell variables must not split an allocated path. A scratch inside the source
# would leave mutation residue there on SIGKILL, so refuse it before allocation.
TMPDIR="$(python3 - "$ROOT" "${TMPDIR:-/tmp}" <<'PY'
from pathlib import Path
import sys
root, parent = (Path(x).resolve() for x in sys.argv[1:])
if any(c in str(parent) for c in '\r\n') or parent == root or root in parent.parents:
    raise SystemExit('FAIL: Linux test scratch must be outside the source, without line breaks')
print(parent)
PY
)"
# Keep allocation, child temporary files and cleanup bound to the same directory after cd.
export TMPDIR
SCRATCH=""
trap 'if [[ -n "$SCRATCH" ]]; then rm -rf "$SCRATCH"; fi' EXIT
SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/assay-linux-gate.XXXXXX")"
# Cargo's intermediate artifacts can live separately from target/. Bind both to this fixture.
export CARGO_TARGET_DIR="$SCRATCH/target" CARGO_BUILD_BUILD_DIR="$SCRATCH/target" CARGO_NET_OFFLINE=true
# Unavailable tooling is a test failure, not a green test that checked nothing. The gate itself
# retains its ordinary partial-coverage behavior; this harness explicitly controls that option.
export ASSAY_LINUX_CHECK_REQUIRE_FULL=1
# Require the repository's pinned compiler to exist already; never auto-install it from this test.
RUSTUP_TOOLCHAIN="$(python3 - "$ROOT/rust-toolchain.toml" <<'PYTOOL'
import sys, tomllib
with open(sys.argv[1], 'rb') as stream:
    print(tomllib.load(stream)['toolchain']['channel'])
PYTOOL
)"
export RUSTUP_TOOLCHAIN
installed_toolchains="$(rustup toolchain list)"
if ! grep -Eq "^${RUSTUP_TOOLCHAIN//./\\.}(-[^ ]+)?( |$)" <<< "$installed_toolchains"; then
  echo "FAIL: pinned toolchain $RUSTUP_TOOLCHAIN is unavailable; install it outside the self-test"
  exit 1
fi
mkdir -p "$SCRATCH/scripts/ci/lib" "$SCRATCH/scripts/precommit"
for path in scripts/ci/check-linux.sh scripts/ci/lib/process_group.py \
  scripts/precommit/run-with-timeout.py scripts/precommit/run-with-timeout.sh rust-toolchain.toml; do
  cp "$ROOT/$path" "$SCRATCH/$path"
done
# Never let fixture Git run the user's hooks or load global Git configuration.
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
git -c core.hooksPath=/dev/null init -q "$SCRATCH"
cd "$SCRATCH"
printf '[workspace]\nmembers = ["crates/*"]\nresolver = "2"\n' > Cargo.toml
crate() {
  local name="$1"
  mkdir -p "crates/$name/src"
  printf '[package]\nname = "%s"\nversion = "0.0.0"\nedition = "2021"\n' "$name" > "crates/$name/Cargo.toml"
}
crate assay-runner-linux
printf 'pub fn runner() {}\n' > crates/assay-runner-linux/src/lib.rs
crate assay-monitor
printf '#[cfg(target_os = "linux")]\npub fn linux_only() {}\n' > crates/assay-monitor/src/lib.rs
printf 'fn main() {}\n' > crates/assay-monitor/build.rs

FAILURES=0
LAST_OUT=""
check() {
  local name="$1" want="$2" result=0
  LAST_OUT="$(bash scripts/ci/check-linux.sh 2>&1)" || result=$?
  if [[ "$result" != "$want" ]]; then
    printf 'FAIL  %s: exit %s, wanted %s\n%s\n' "$name" "$result" "$want" "$LAST_OUT"
    FAILURES=$((FAILURES + 1))
  else
    printf 'ok    %s\n' "$name"
  fi
}
contains() {
  if grep -Fq -- "$2" <<< "$LAST_OUT"; then
    printf 'ok    %s\n' "$1"
  else
    printf "FAIL  %s: output lacks '%s'\n%s\n" "$1" "$2" "$LAST_OUT"
    FAILURES=$((FAILURES + 1))
  fi
}
check 'clean fixture has full Linux coverage' 0
contains 'named Linux-only crate is checked' 'assay-runner-linux'
contains 'source-discovered crate is checked' 'assay-monitor'
# Stop before mutation if the clean control failed, e.g. the Linux std target is unavailable.
[[ "$FAILURES" = 0 ]] || exit 1
cp crates/assay-monitor/src/lib.rs clean-monitor.rs
cat >> crates/assay-monitor/src/lib.rs <<'RS'
#[cfg(target_os = "linux")]
fn deliberately_broken_for_the_gate_test() {
    let _ = A_VALUE_THAT_DOES_NOT_EXIST;
}
RS
check 'a cfg(linux) compile error fails the gate' 1
contains 'the compiler locates the planted error' 'A_VALUE_THAT_DOES_NOT_EXIST'
contains 'the gate identifies the broken crate' 'assay-monitor'
# Crucially, run this BEFORE restoring the source. A control after restoration proves nothing
# about the planted cfg. An explicit host target cannot inherit CARGO_BUILD_TARGET.
host="$(rustc -vV | sed -n 's/^host: //p')"
[[ -n "$host" ]] || { echo 'FAIL: rustc did not report its host'; exit 1; }
host_rc=0
cargo check --offline -p assay-monitor --target "$host" --quiet > host.log 2>&1 || host_rc=$?
case "$host" in
  *-linux-*)
    if [[ "$host_rc" = 0 ]] || ! grep -Fq 'A_VALUE_THAT_DOES_NOT_EXIST' host.log; then
      echo 'FAIL: the Linux host control did not see the planted error'; cat host.log; FAILURES=$((FAILURES + 1))
    else echo 'ok    Linux host control sees the still-planted cfg(linux) error'; fi ;;
  *)
    if [[ "$host_rc" != 0 ]]; then
      echo 'FAIL: the non-Linux host control failed with the cfg(linux) error still planted'; cat host.log; FAILURES=$((FAILURES + 1))
    else echo 'ok    non-Linux host control passes with the cfg(linux) error still planted'; fi ;;
esac
cp clean-monitor.rs crates/assay-monitor/src/lib.rs

# A build-script failure is not a source diagnostic. Partial mode reports what was unchecked;
# REQUIRE_FULL promotes exactly that same fixture to a failing gate, on Linux as well as macOS.
printf 'fn main() { panic!("fixture build unavailable"); }\n' > crates/assay-monitor/build.rs
ASSAY_LINUX_CHECK_REQUIRE_FULL=0 check 'partial mode reports an unavailable build' 0
contains 'partial coverage is named' 'unchecked'
ASSAY_LINUX_CHECK_REQUIRE_FULL=1 check 'strict mode refuses the same unavailable build' 1
printf 'fn main() {}\n' > crates/assay-monitor/build.rs

# A newly added cfg crate must be discovered without editing the gate's crate list.
crate assay-discovered
printf '#[cfg(target_os = "linux")]\nfn new_crate() { let _ = NEW_LINUX_ERROR; }\n' > crates/assay-discovered/src/lib.rs
check 'a new cfg crate is discovered' 1
contains 'new crate is named' 'assay-discovered'
contains 'new crate compiler error is observed' 'NEW_LINUX_ERROR'
[[ "$FAILURES" = 0 ]] || { echo "$FAILURES linux-gate case(s) failed"; exit 1; }
echo 'linux compile gate: all cases pass (offline fixture, real compiler)'
