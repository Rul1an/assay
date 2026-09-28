#!/usr/bin/env python3
"""Real consumer compatibility; shared by live checks and mutation inputs."""
import re
import sys
from pathlib import Path

from release_heading import RELEASE_HEADING

dependabot = Path(sys.argv[1]).read_text(encoding="utf-8")
pinned = Path(sys.argv[2]).read_text(encoding="utf-8")
changelog = Path(sys.argv[3]).read_text(encoding="utf-8")
errors = []
if 'assay-dev/assay-action' in dependabot:
    errors.append("Dependabot ignore names assay-dev/assay-action; want Rul1an/assay-action")
if 'dependency-name: "Rul1an/assay-action"' not in dependabot:
    errors.append("Dependabot does not ignore Rul1an/assay-action")
if ".github/assay-action-pin" not in pinned or "not a second place to change" not in pinned:
    errors.append("PINNED-ACTIONS.md does not record the pin-file exception")
if "Do not move floating `v3`" not in pinned or "Do not move frozen `v2`" not in pinned:
    errors.append("PINNED-ACTIONS.md does not record Assay-side rollback")
unreleased_start = changelog.find("## [Unreleased]")
if unreleased_start < 0:
    errors.append("CHANGELOG.md has no Unreleased section")
next_h2 = re.search(r"^## .+$", changelog[unreleased_start + 1 :], re.MULTILINE)
first_release = -1
if next_h2 is not None:
    first_release = unreleased_start + 1 + next_h2.start()
    release_heading = next_h2.group(0)
    if RELEASE_HEADING.fullmatch(release_heading) is None:
        errors.append("CHANGELOG Unreleased is not followed by a dated semver release")
claims = (
    "mixed Action migration",
    "literal `false`",
    "sandbox-command",
    "v3.0.1 to v3.0.2",
    "not measured",
)
claim_positions = []
for needle in claims:
    position = changelog.find(needle)
    claim_positions.append(position)
    if position < 0:
        errors.append(f"CHANGELOG history does not name {needle!r}")
if first_release < 0:
    errors.append("CHANGELOG.md has no numbered release history")
elif all(position >= 0 for position in claim_positions):
    active = [unreleased_start < position < first_release for position in claim_positions]
    if any(active) and not all(active):
        errors.append("CHANGELOG Action migration claims are split across active and released history")
    elif not any(active) and any(position < first_release for position in claim_positions):
        errors.append("CHANGELOG Action migration claims precede active and released history")
if errors:
    raise SystemExit("; ".join(errors))
