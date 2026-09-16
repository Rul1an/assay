# shellcheck shell=bash

# One helper reports declaration sites and workspace lockfiles. The counts are consumed as
# data rather than trusted: a helper that dies, or that stops emitting a count, must not read
# as a clean sweep. A `fail` inside the process substitution below would run in its subshell and
# never reach the caller, which is why the guards live out here instead.
if ! type note >/dev/null 2>&1; then
  note() { :; }
fi

root_checked=""
crate_checked=""
lock_checked=""
while IFS=$'\t' read -r kind value; do
  case "$kind" in
    root_count)  root_checked="$value" ;;
    crate_count) crate_checked="$value" ;;
    lock_count)  lock_checked="$value" ;;
    fail)        fail "$value" ;;
  esac
done < <(python3 "${ROOT:-.}/scripts/ci/check_internal_dep_versions.py" || true)

if [ -z "$root_checked" ]; then
  fail "internal dependency check reported no root count; the enumeration is broken"
elif [ "$root_checked" -eq 0 ]; then
  fail "no internal path dependencies found in [workspace.dependencies]; the enumeration is broken"
else
  note "  checked $root_checked root declaration(s)"
fi

if [ -z "$crate_checked" ]; then
  fail "internal dependency check reported no crate count; the enumeration is broken"
elif [ "$crate_checked" -eq 0 ]; then
  fail "no crate-level path dependencies on workspace members found; the enumeration is broken"
else
  note "  checked $crate_checked crate-level declaration(s)"
fi

if [ -z "$lock_checked" ]; then
  fail "internal dependency check reported no lock count; the enumeration is broken"
elif [ "$lock_checked" -eq 0 ]; then
  fail "no lockfiles pinning workspace members found; the enumeration is broken"
else
  note "  checked $lock_checked lockfile(s) pinning workspace members"
fi
