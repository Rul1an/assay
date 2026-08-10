#!/usr/bin/env bash
# Install cargo-deny at the pinned version, from a toolchain this job actually has.
#
# The version lives here and nowhere else. Two workflows install cargo-deny -- the dependency
# security job in ci.yml and the pre-commit lint job in kernel-matrix.yml -- and before this script
# each ran its own `cargo install --locked cargo-deny`, resolving whatever was latest at run time.
# Two jobs in one pull request could therefore enforce two different rule sets, and a new cargo-deny
# release could redden a pull request that changed nothing related.
#
# `--locked` did not prevent that, and it is worth being explicit about why, because it reads like a
# pin: it pins cargo-deny's *own dependency versions* from its published lockfile, not the version
# of cargo-deny being installed. Anyone removing `--version` below because `--locked` looks
# sufficient will reintroduce the drift.
set -euo pipefail

CARGO_DENY_VERSION="0.20.2"

# Select the toolchain explicitly rather than letting rustup choose.
#
# Both jobs run inside the repository, where `rust-toolchain.toml` pins 1.96.0. rustup honours that
# file, so `cargo install` selected 1.96.0 -- a toolchain neither job installs. The lint job installs
# `stable` (observed as 1.97.1 in its own log) and the dependency job installs nothing at all, so
# whether the step worked depended on what the runner image happened to carry:
#
#   error: 'cargo' is not installed for the toolchain '1.96.0-x86_64-unknown-linux-gnu'
#
# The same branch passed at 11:43 and failed at 11:48 on 2026-08-10 with no code change. The log
# named 1.97.1 while 1.96.0 ran, which is why reading the log gave the wrong answer about what was
# selected. RUSTUP_TOOLCHAIN overrides the file, so the selection is now stated rather than inferred.
#
# An inherited value wins. ci.yml's dependency job states the selection once for every cargo command
# it runs; overriding that here would make the job's statement decorative for this step, and would
# make `stable` a value written in two places that have to agree -- the defect this script exists to
# remove. `stable` below is the fallback for a caller that states nothing, which is the lint job in
# kernel-matrix.yml and anyone running the script by hand.
export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}"

echo "installing cargo-deny ${CARGO_DENY_VERSION} using toolchain ${RUSTUP_TOOLCHAIN}"
cargo install --locked --version "${CARGO_DENY_VERSION}" cargo-deny

# Echo the resolved version and hold it against the pin, so the number in the log is one that was
# checked rather than one that was printed. An install that silently produced a different version
# would otherwise leave a log that looks like evidence of the pin.
resolved="$(cargo-deny --version)"
echo "cargo-deny resolved: ${resolved}"
if [[ "${resolved}" != "cargo-deny ${CARGO_DENY_VERSION}" ]]; then
  echo "cargo-deny reports '${resolved}', which is not the pinned ${CARGO_DENY_VERSION}" >&2
  echo "the pin lives in scripts/ci/install-cargo-deny.sh and is the only place to change it" >&2
  exit 1
fi
