#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
WRITER="${SCRIPT_DIR}/write_sha256_sidecar.sh"
CI_WORKFLOW="${REPO_ROOT}/.github/workflows/ci.yml"
RELEASE_WORKFLOW="${REPO_ROOT}/.github/workflows/release.yml"
MCPB_BUILDER="${SCRIPT_DIR}/build_mcpb_bundle.sh"

tmp_root="$(mktemp -d)"
trap 'rm -rf "$tmp_root"' EXIT

release_dir="${tmp_root}/release"
mkdir -p "$release_dir"
asset="${release_dir}/assay-v9.9.9-release-proof-kit.tar.gz"
printf 'release asset fixture\n' >"$asset"

bash "$WRITER" "$asset"

if command -v sha256sum >/dev/null 2>&1; then
  digest="$(sha256sum "$asset" | awk '{print $1}')"
else
  digest="$(shasum -a 256 "$asset" | awk '{print $1}')"
fi

printf '%s  %s\n' "$digest" "$(basename "$asset")" >"${tmp_root}/expected.sha256"
cmp "${tmp_root}/expected.sha256" "${asset}.sha256"

(
  cd "$release_dir"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum -c "$(basename "${asset}.sha256")" >/dev/null
  else
    shasum -a 256 -c "$(basename "${asset}.sha256")" >/dev/null
  fi
)

python3 - "$CI_WORKFLOW" "$RELEASE_WORKFLOW" "$MCPB_BUILDER" <<'PY'
import re
import sys
from pathlib import Path

ci_path, release_path, mcpb_path = map(Path, sys.argv[1:])
ci_text = ci_path.read_text()
release_text = release_path.read_text()
mcpb_text = mcpb_path.read_text()

workflow_calls = (
    'bash scripts/ci/write_sha256_sidecar.sh "dist/${ARCHIVE_NAME}.${{ matrix.archive }}"',
    'bash "${GITHUB_WORKSPACE}/scripts/ci/write_sha256_sidecar.sh" "${ARCHIVE_NAME}.tar.gz"',
    'bash scripts/ci/write_sha256_sidecar.sh "release/assay-${VERSION}-sbom-cyclonedx.tar.gz"',
    'bash scripts/ci/write_sha256_sidecar.sh "${OUT_SUMMARY}"',
    'bash scripts/ci/write_sha256_sidecar.sh "${OUT_ARCHIVE}"',
)
mcpb_call = 'bash "${SCRIPT_DIR}/write_sha256_sidecar.sh" "$OUTPUT"'
APPROVED_WRITER_CALLS = frozenset((*workflow_calls, mcpb_call))
# Closed form: a .sha256 path is a producer unless it is an approved writer
# call or a verification. Live copy/read/download/prose lines are pinned so a
# new .sha256 line is refused regardless of the command that writes it.
SHA256_PATH = re.compile(r"(?i)\.sha256\b")
VERIFY_INVOCATION = re.compile(r"(?i)(?:sha256sum|shasum\s+-a\s+256)\s+-c\b")
LIVE_SHA256_CONSUMERS = frozenset(
    (
        r'find artifacts -type f \( -name "*.tar.gz" -o -name "*.zip" -o -name "*.sha256" \) -exec cp {} release/ \;',
        'MCPB_SHA="$(cut -d\' \' -f1 "${MCPB_FILE}.sha256")"',
        "PROVENANCE_SUMMARY_SHA256: release/assay-${{ needs.release-contract.outputs.version }}-release-provenance.json.sha256",
        "Verify your download with the `.sha256` files.",
        '"assay-mcp-server-${VERSION}-x86_64-unknown-linux-gnu.tar.gz.sha256" \\',
        '"assay-mcp-server-${VERSION}-aarch64-unknown-linux-gnu.tar.gz.sha256" \\',
        '"assay-${VERSION}-sbom-cyclonedx.tar.gz.sha256"',
        '"$archive" "${archive}.sha256"',
    )
)
PRODUCER_MSG = "release checksum producer bypasses write_sha256_sidecar.sh"


def is_approved_writer_call(line):
    return line.strip() in APPROVED_WRITER_CALLS


def is_verification(line):
    return bool(VERIFY_INVOCATION.search(line))


def is_checksum_producer(line):
    if not SHA256_PATH.search(line):
        return False
    stripped = line.strip()
    if is_approved_writer_call(stripped) or is_verification(line):
        return False
    if stripped in LIVE_SHA256_CONSUMERS:
        return False
    return True


def active_lines(text):
    return [
        line.strip()
        for line in text.splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    ]


def validate_wiring(workflow, mcpb):
    workflow_active = active_lines(workflow)
    mcpb_active = active_lines(mcpb)
    for expected in workflow_calls:
        if workflow_active.count(expected) != 1:
            raise ValueError(f"missing or duplicate active release checksum call: {expected}")
    if mcpb_active.count(mcpb_call) != 1:
        raise ValueError("missing or duplicate active MCPB checksum call")
    if any(is_checksum_producer(line) for line in workflow_active + mcpb_active):
        raise ValueError(PRODUCER_MSG)


def expect_classified(label, line, want_producer):
    got = bool(is_checksum_producer(line))
    if got != want_producer:
        raise SystemExit(
            f"{label}: is_checksum_producer({line!r}) is {got}, want {want_producer}"
        )


def expect_producer_refused(label, mutated):
    try:
        validate_wiring(mutated, mcpb_text)
    except ValueError as exc:
        if str(exc) != PRODUCER_MSG:
            raise SystemExit(f"{label}: refused for the wrong reason: {exc}") from exc
    else:
        raise SystemExit(f"{label}: producer mutation passed the wiring contract")


PRODUCER_MUTATIONS = (
    ("(a) sha256sum redirect", 'sha256sum "$f" > "$f.sha256"'),
    ("(b) Get-FileHash/Out-File", 'Get-FileHash $f | Out-File "$f.sha256"'),
    ("(b) Get-FileHash/Set-Content", 'Get-FileHash $f | Set-Content "$f.sha256"'),
    ("(b) Get-FileHash/Add-Content", 'Get-FileHash $f | Add-Content "$f.sha256"'),
    ("(b) Get-FileHash/out-file", 'Get-FileHash $f | out-file "$f.sha256"'),
    ("sha256sum tee", 'sha256sum "$f" | tee "$f.sha256"'),
    ("sha256sum -b append", 'sha256sum -b "$f" >> "$f.sha256"'),
)
MUSE_BYPASSES = PRODUCER_MUTATIONS[2:5]
VERIFY_LINES = (
    ("(d) sha256sum -c", 'sha256sum -c "${archive}.sha256"'),
    ("(d) shasum -a 256 -c", "shasum -a 256 -c file.sha256"),
)

# One predicate: live check and mutation list both call is_checksum_producer.
for label, line in PRODUCER_MUTATIONS:
    expect_classified(label, line, True)
    expect_producer_refused(label, release_text + "\n" + line + "\n")
for label, line in VERIFY_LINES:
    expect_classified(label, line, False)
for call in workflow_calls:
    expect_classified(f"shared writer {call}", call, False)
expect_classified("mcpb writer", mcpb_call, False)

# (c) a commented shared-writer call is still an inactive producer.
provenance_call = workflow_calls[3]
mutated = release_text.replace(
    f"          {provenance_call}",
    f"          # {provenance_call}",
    1,
)
try:
    validate_wiring(mutated, mcpb_text)
except ValueError:
    pass
else:
    raise SystemExit("commented checksum producer passed the wiring contract")

# (d) live workflows: verification invocations are not producers.
validate_wiring(release_text, mcpb_text)

live_consumers = frozenset(
    line
    for line in active_lines(release_text) + active_lines(mcpb_text)
    if SHA256_PATH.search(line)
    and not is_verification(line)
    and not is_approved_writer_call(line)
)
if live_consumers != LIVE_SHA256_CONSUMERS:
    raise SystemExit(
        "LIVE_SHA256_CONSUMERS drifted: "
        f"{sorted(live_consumers ^ LIVE_SHA256_CONSUMERS)!r}"
    )

# Must-bite: enumerating write sinks (the previous predicate) lets Muse's
# three bypasses survive. Closed form must not name those sinks.
if {"sidecar_write", "checksum_cmd"} & set(is_checksum_producer.__code__.co_names):
    raise SystemExit("closed-form predicate still uses the sink-enumeration names")


def sink_enumerated_is_checksum_producer(line):
    checksum_cmd = re.compile(r"(shasum -a 256|sha256sum|Get-FileHash|Out-File)")
    verify_invocation = re.compile(r"(?:sha256sum|shasum -a 256)\s+-c\b")
    sidecar_write = re.compile(r"(?:>>|>|-o\b|\btee\b|\bOut-File\b).*\.sha256")
    if verify_invocation.search(line):
        return False
    return bool(checksum_cmd.search(line) and sidecar_write.search(line))


closed_form = is_checksum_producer
is_checksum_producer = sink_enumerated_is_checksum_producer
for label, line in MUSE_BYPASSES:
    if sink_enumerated_is_checksum_producer(line):
        raise SystemExit(f"must-bite setup: sink list already catches {label}")
    try:
        expect_classified(label, line, True)
    except SystemExit as exc:
        if f"{label}: is_checksum_producer({line!r}) is False, want True" not in str(
            exc
        ):
            raise
    else:
        raise SystemExit(f"must-bite: neutered predicate still refused {label}")
is_checksum_producer = closed_form
for label, line in MUSE_BYPASSES:
    expect_classified(label, line, True)

start = ci_text.index("  release-asset-contract:")
remaining = ci_text[start + 1 :]
next_job = re.search(r"^  [a-zA-Z0-9_-]+:$", remaining, re.MULTILINE)
section = ci_text[start : start + 1 + next_job.start()] if next_job else ci_text[start:]
matrix_contract = (
    "name: Release asset contract (${{ matrix.os }})",
    "runs-on: ${{ matrix.os }}",
    "- ubuntu-latest",
    "- windows-latest",
)


def validate_matrix(text):
    active = active_lines(text)
    for required in matrix_contract:
        if active.count(required) != 1:
            raise ValueError(f"release asset contract matrix is missing: {required}")
    if any(line.split(":", 1)[0] == "exclude" for line in active):
        raise ValueError("release asset contract matrix may not exclude an OS")


validate_matrix(section)
matrix_mutations = [
    section.replace("          - windows-latest", "          # - windows-latest", 1),
    section.replace(
        "          - windows-latest",
        "          - windows-latest\n        exclude:\n          - os: windows-latest",
        1,
    ),
]
for mutated in matrix_mutations:
    try:
        validate_matrix(mutated)
    except ValueError:
        continue
    raise SystemExit("release asset contract matrix bypass mutation passed")
PY

echo "sha256 sidecar writer tests passed"
