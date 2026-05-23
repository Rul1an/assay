#!/usr/bin/env bash
# Guardrail: assay-cli must be packageable to crates.io.
#
# Background. PR #1325 (Slice 6B) introduced direct dependencies in
# crates/assay-cli/Cargo.toml on the internal `assay-runner-{schema,core,linux}`
# crates, all of which are `publish = false`. cargo publish only ran on tag
# push, so the failure mode surfaced after merge during the v3.11.0 release
# attempt as `no matching package named "assay-runner-core" found`.
#
# This script statically verifies the shape that matters at PR time:
# every dependency in crates/assay-cli/Cargo.toml that targets a workspace
# crate marked `publish = false` MUST be `optional = true`. Optional deps
# stay out of the published manifest unless their gating feature is
# activated, and the publish pipeline activates only `tui,sim` for
# `assay-cli` (no `runner`).
#
# Fail-fast policy: if a future PR adds a non-optional dep on a publish=false
# crate to assay-cli, this script fails on the PR — not after tag push.

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

python3 - <<'PY'
import re
import sys
from pathlib import Path

ROOT = Path(".")
CLI_MANIFEST = ROOT / "crates" / "assay-cli" / "Cargo.toml"

# 1. Discover every workspace member's publish-ability.
workspace_root = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
members_block = re.search(
    r"(?ms)^members\s*=\s*\[(.+?)\]",
    workspace_root,
)
if not members_block:
    sys.exit("ERROR: could not find workspace members in root Cargo.toml")

members = [
    m.strip(' "')
    for m in re.findall(r'"([^"]+)"', members_block.group(1))
]

publish_false = set()
for member_path in members:
    manifest = ROOT / member_path / "Cargo.toml"
    if not manifest.exists():
        continue
    text = manifest.read_text(encoding="utf-8")
    if re.search(r"(?m)^\s*publish\s*=\s*false\s*$", text):
        # Extract the crate name from the [package] table.
        name_match = re.search(r'(?ms)^\[package\][^\[]*?\bname\s*=\s*"([^"]+)"', text)
        if name_match:
            publish_false.add(name_match.group(1))

if not publish_false:
    sys.exit("ERROR: detected no publish = false workspace crates — heuristic broken?")

# 2. Parse assay-cli's [dependencies] table for entries that reference any
#    of those crates and are NOT marked optional = true.
cli_text = CLI_MANIFEST.read_text(encoding="utf-8")

# Restrict to the [dependencies] table; skip [dev-dependencies] and target-
# specific tables (those are not published into the runtime dep set in a way
# that triggers the failure mode we care about).
dep_section_match = re.search(
    r"(?ms)^\[dependencies\]\s*$(.+?)(?=^\[|\Z)",
    cli_text,
)
if not dep_section_match:
    sys.exit("ERROR: could not find [dependencies] in assay-cli/Cargo.toml")

dep_section = dep_section_match.group(1)

bad = []
for crate in sorted(publish_false):
    # Match lines like:
    #   assay-runner-core = { workspace = true }
    #   assay-runner-core = { workspace = true, optional = true }
    #   assay-runner-core.workspace = true
    pattern = re.compile(
        rf"(?m)^\s*{re.escape(crate)}\s*(?:=\s*\{{(?P<inline>[^}}]*)\}}|\.workspace\s*=\s*true)\s*$"
    )
    for match in pattern.finditer(dep_section):
        inline = match.group("inline") or ""
        if "optional" in inline and re.search(r"optional\s*=\s*true", inline):
            continue
        bad.append((crate, match.group(0).strip()))

if bad:
    print("Publish-shape guardrail FAILED.")
    print()
    print("crates/assay-cli/Cargo.toml has non-optional dependencies on workspace")
    print("crates that are publish = false. cargo publish for assay-cli will")
    print("fail at release time with 'no matching package named ... found'.")
    print()
    print("Either mark each offending dep `optional = true` behind a non-default")
    print("feature, or remove the dependency. See CHANGELOG entry for v3.11.1 and")
    print("the `runner` feature in crates/assay-cli/Cargo.toml for the canonical")
    print("pattern.")
    print()
    print("Offending entries:")
    for crate, line in bad:
        print(f"  - {crate}: {line}")
    sys.exit(1)

print("Publish-shape guardrail OK: assay-cli has no non-optional deps on")
print(f"publish = false crates ({len(publish_false)} workspace crates checked).")
PY
