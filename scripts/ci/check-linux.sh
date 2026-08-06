#!/usr/bin/env bash
# Linux-only compile guard for macOS dev: catches `cfg(target_os = "linux")` compile errors.
#
# This gate previously could not fail. Both branches ended:
#
#     cargo check --workspace ... || { echo "WARN: ... Relying on CI."; return 0; }
#
# so a timeout, a missing toolchain, a third-party build script, and *your code not compiling on
# Linux* were one outcome: WARN, and pre-push reported Passed. #2074 pushed four `E0425`s through
# it and found out on the delegated host (#2076).
#
# Worse, on a stock macOS box the workspace check fails every time -- `ring` and `aws-lc-sys` build
# scripts need a C cross-compiler -- so it took the return-0 path every time and had never been able
# to report anything.
#
# The distinction this restores is the one the 2026 supply-chain literature keeps naming: a step
# that logs "could not verify" and leaves the pipeline green is a silent pass. "Could not check" and
# "your code is broken" must not be spelled the same way.
#
#   FAIL   a compiler error in our source, on the Linux target
#   WARN   a cause that is not our code (build script, toolchain, timeout) -- and the crates it
#          left unchecked are named, because a gate that cannot check everything must say what it
#          did not check
#
# ASSAY_LINUX_CHECK_REQUIRE_FULL=1 promotes incomplete coverage to a failure, for a host that can
# cross-compile everything and wants the stronger guarantee.
#
# The `multipass` VM mode this script used to carry is gone, and it is worth saying why rather than
# leaving a reader to wonder. Its clippy invocation ended the same way:
#
#     cargo clippy --locked --workspace --all-targets -- -D warnings || {
#       echo "WARN: Linux check timed out or failed. Relying on CI."; return 0; }
#
# so it could not fail either, and the `target` case wrapped it in a second fail-open on top. Two
# paths with one defect means fixing it in one of them leaves the other. The cross-target path is
# the one that runs by default and needs no VM, so it is the one that was kept and repaired.

set -uo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root" || exit 1

TARGET="x86_64-unknown-linux-gnu"
TIMEOUT_SECS="${ASSAY_LINUX_TARGET_TIMEOUT:-300}"
REQUIRE_FULL="${ASSAY_LINUX_CHECK_REQUIRE_FULL:-0}"

# A compiler diagnostic against our source is path-prefixed by `--message-format=short`:
#
#   crates/assay-monitor/src/loader.rs:440:19: error[E0425]: cannot find value ...
#
# A cause that is not our code is not:
#
#   error: failed to run custom build command for `ring v0.17.14`
#
# That prefix is the whole classifier. Deliberately anchored, so an error mentioning a path in its
# message body is not mistaken for one located there.
OURS='^(crates|assay-python-sdk)/[^:]+:[0-9]+:[0-9]+: error'

# The crates that actually carry Linux-gated code, derived from the source rather than listed.
#
# A hand-kept list is one more thing to drift, and the drift is silent in the dangerous direction: a
# new crate with `cfg(target_os = "linux")` code would simply not be checked, and nothing would say
# so. `assay-runner-linux` is included by name because the whole crate is Linux-only and may carry
# no `cfg` attribute at all.
linux_crates() {
  {
    grep -rl 'cfg(target_os = "linux")' --include='*.rs' crates assay-python-sdk 2>/dev/null \
      | cut -d/ -f2
    echo "assay-runner-linux"
  } | sort -u
}

if ! rustup target list --installed 2>/dev/null | grep -qx "$TARGET"; then
  rustup target add "$TARGET" >/dev/null 2>&1 || true
fi
if ! rustup target list --installed 2>/dev/null | grep -qx "$TARGET"; then
  echo "WARN: the $TARGET std component is not installed and could not be added."
  echo "      Nothing was checked. This is a 'could not check', not a pass."
  [ "$REQUIRE_FULL" = "1" ] && exit 1
  exit 0
fi

timeout_bin=""
if [ "$TIMEOUT_SECS" -gt 0 ] 2>/dev/null; then
  command -v timeout >/dev/null 2>&1 && timeout_bin=timeout
  [ -z "$timeout_bin" ] && command -v gtimeout >/dev/null 2>&1 && timeout_bin=gtimeout
fi

log="$(mktemp)"
trap 'rm -f "$log"' EXIT

checked=(); broken=(); unchecked=()

for crate in $(linux_crates); do
  if [ -n "$timeout_bin" ]; then
    "$timeout_bin" "$TIMEOUT_SECS" cargo check -p "$crate" --all-targets --target "$TARGET" \
      --message-format=short > "$log" 2>&1
  else
    cargo check -p "$crate" --all-targets --target "$TARGET" --message-format=short > "$log" 2>&1
  fi
  status=$?

  if grep -qE "$OURS" "$log"; then
    broken+=("$crate")
    echo "FAIL $crate"
    grep -E "$OURS" "$log" | sed 's/^/     /'
  elif [ "$status" -ne 0 ]; then
    # Non-zero with no diagnostic against our source. The cause is a build script, the toolchain,
    # or the timeout -- none of which is evidence about our code, and none of which may be reported
    # as if it were.
    reason="$(grep -m1 -E '^error' "$log" || echo 'no diagnostic captured')"
    unchecked+=("$crate")
    echo "SKIP $crate -- could not be checked: ${reason}"
  else
    checked+=("$crate")
    echo "ok   $crate"
  fi
done

total=$(( ${#checked[@]} + ${#broken[@]} + ${#unchecked[@]} ))
echo
echo "Linux cross-target: ${#checked[@]} ok, ${#broken[@]} broken, ${#unchecked[@]} unchecked, of ${total} crate(s) carrying Linux-gated code."

if [ ${#broken[@]} -gt 0 ]; then
  echo "error: ${broken[*]} do not compile for ${TARGET}." >&2
  exit 1
fi

if [ ${#unchecked[@]} -gt 0 ]; then
  echo "WARN: not checked, so this run says nothing about them: ${unchecked[*]}"
  if [ "$REQUIRE_FULL" = "1" ]; then
    echo "error: ASSAY_LINUX_CHECK_REQUIRE_FULL=1 and coverage is incomplete." >&2
    exit 1
  fi
fi

exit 0
