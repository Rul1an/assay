#!/usr/bin/env bash
set -euo pipefail

# Public crate contract: every workspace member whose package metadata
# allows publishing must appear here, and vice versa. This is the
# release-truth-line allow-list; the policy script enforces it against
# both `Cargo.toml` metadata and the publish_idempotent.sh `CRATES`
# array.
#
# The `assay-runner-{schema,linux,core}` crates are registered here as of
# v3.11.3, with their package descriptions explicitly framing them as
# internal/experimental substrate (no standalone product guarantee,
# semver follows the Assay workspace, intentionally undocumented for
# third-party use). They are published because `assay-cli` depends on
# them; cargo publish requires every dep in `[dependencies]` to be
# resolvable from crates.io regardless of feature activation, so
# keeping required runner crates `publish = false` made `assay-cli` itself
# unpublishable
# (see CHANGELOG entries for v3.11.0, v3.11.1, v3.11.2, v3.11.3).
#
# Adding any new public crate here is a deliberate public-surface
# decision; the PR that does so must update package descriptions and
# (when relevant) docs/contributing/WAVE0-GATES.md.
public_crates=(
  assay-common
  assay-registry
  assay-evidence
  assay-core
  assay-metrics
  assay-policy
  assay-mcp-server
  assay-monitor
  assay-runner-schema
  assay-runner-linux
  assay-runner-core
  assay-sim
  assay-cli
)

non_crates_io_crates=(
  assay-adapter-api
  assay-adapter-acp
  assay-adapter-a2a
  assay-adapter-ucp
  assay-it
  assay-ebpf
  assay-xtask
)

command -v cargo >/dev/null 2>&1 || { echo "cargo missing" >&2; exit 1; }
command -v jq >/dev/null 2>&1 || { echo "jq missing" >&2; exit 1; }
command -v python3 >/dev/null 2>&1 || { echo "python3 missing" >&2; exit 1; }

join_lines() {
  printf '%s\n' "$@" | sort
}

expected_public="$(join_lines "${public_crates[@]}")"
metadata_json="$(cargo metadata --no-deps --format-version 1)"

metadata_public="$(
  printf '%s\n' "$metadata_json" |
    jq -r '.packages[] | select((.publish // ["default"]) != []) | .name' |
    sort
)"

if [[ "$metadata_public" != "$expected_public" ]]; then
  echo "FAIL: Cargo.toml publish policy does not match public crate contract." >&2
  echo "Expected public crates:" >&2
  printf '%s\n' "$expected_public" >&2
  echo "Metadata-publishable crates:" >&2
  printf '%s\n' "$metadata_public" >&2
  exit 1
fi

for crate in "${non_crates_io_crates[@]}"; do
  publish_value="$(
    printf '%s\n' "$metadata_json" |
      jq -r --arg crate "$crate" '.packages[] | select(.name == $crate) | (.publish // ["default"]) | @json'
  )"
  if [[ "$publish_value" != "[]" ]]; then
    echo "FAIL: ${crate} must set publish = false." >&2
    exit 1
  fi
done

publish_script_crates="$(
  python3 - <<'PY'
from pathlib import Path
import re

text = Path("scripts/ci/publish_idempotent.sh").read_text(encoding="utf-8")
match = re.search(r"(?ms)^CRATES=\(\n(?P<body>.*?)^\)", text)
if not match:
    raise SystemExit("CRATES array not found")
for line in match.group("body").splitlines():
    line = line.strip()
    if not line or line.startswith("#"):
        continue
    m = re.match(r'"([^"]+)"$', line)
    if not m:
        raise SystemExit(f"unexpected CRATES entry: {line}")
    print(m.group(1))
PY
)"

if [[ "$(printf '%s\n' "$publish_script_crates" | sort)" != "$expected_public" ]]; then
  echo "FAIL: publish_idempotent.sh CRATES must match public crate contract." >&2
  echo "Expected public crates:" >&2
  printf '%s\n' "$expected_public" >&2
  echo "publish_idempotent.sh crates:" >&2
  printf '%s\n' "$publish_script_crates" | sort >&2
  exit 1
fi

echo "Public crate policy is consistent."
