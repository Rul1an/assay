#!/usr/bin/env python3
"""Docs/workflow contract: release runbook may name only executable release.yml surfaces."""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
WORKFLOW = REPO / ".github/workflows/release.yml"
DOCS = REPO / "docs/reference/release.md"

_LSM_SMOKE = "lsm-smoke-test"
_LSM_ITEM_TITLE = "Optional LSM verification"
_LOCAL_LSM_CMD = "scripts/verify_lsm_docker.sh --release-tag vX.Y.Z"
_PYPI_ITEM_TITLE = "PyPI Trusted Publisher"
_PYPI_REPOSITORY = "Rul1an/assay"
_PYPI_WORKFLOW = "release.yml"
_PYPI_ENVIRONMENT = "pypi"
_PYPI_LEGACY_WORKFLOW = "publish.yml"
_PYPI_PUBLISH_ACTION = (
    "pypa/gh-action-pypi-publish@dc37677b2e1c63e2034f94d8a5b11f265b73ba33"
)
_PYPI_LEGACY_REMOVAL_SENTENCE = (
    "Remove every other publisher, including the legacy `publish.yml` identity."
)
_PYPI_SINGLE_PUBLISHER_SENTENCE = (
    "In the `assay-it` project Publishing page, require exactly one GitHub publisher: "
    "repository `Rul1an/assay`, "
    "workflow `release.yml`, environment `pypi`."
)
_PYPI_EMPTY_ENVIRONMENT_SENTENCE = (
    "An empty environment is broader authority and does not match this contract."
)
_CRATES_ITEM_TITLE = "Trusted Publishing"
_CRATES_REPOSITORY = _PYPI_REPOSITORY
_CRATES_WORKFLOW = _PYPI_WORKFLOW
_CRATES_ENVIRONMENT = "crates"
_CRATES_PUBLISH_ACTION = (
    "rust-lang/crates-io-auth-action@c6f97d42243bad5fab37ca0427f495c86d5b1a18"
)
_CRATES_IDENTITY_SENTENCE = (
    "Require, per crate, a GitHub Trusted Publisher: "
    "repository `Rul1an/assay`, "
    "workflow `release.yml`, environment `crates`."
)
_CRATES_LEGACY_REMOVAL_SENTENCE = (
    "Remove every other publisher, including any publisher whose environment is unset."
)
_CRATES_UNSET_ENVIRONMENT_SENTENCE = (
    "An unset environment is broader authority and does not match this contract."
)
_PYPI_RECEIPT_SENTENCE = (
    "compare its expected identity with every "
    "owner-visible PyPI publisher row, and retain a redacted receipt containing only the project, "
    "repository, workflow, environment, publisher count, observation time, and result."
)
_CRATES_RECEIPT_SENTENCE = (
    "compare its expected identity with every "
    "owner-visible crates.io publisher row, and retain a redacted receipt containing only the crate, "
    "repository, workflow, environment, publisher count, observation time, and result. "
    "No credentials. Apply this on every current crates.io crate:"
)
# Environment vocabulary is an allowlist of pinned item sentences, not a
# forbidden-phrase list. Closed-form ownership of the whole item would still pin
# the crate inventory; those lines do not mention environment, so a crate-list
# edit stays green. Both halves share this check so fixing one cannot reintroduce
# the parity overclaim.
_CHECKLIST_ITEM_PREFIX = re.compile(r"^- \[[ x]\] \*\*[^*]+\*\*: ")
_RECEIPT_COMMAND_PREFIX = (
    "Before creating a tag, run `python3 scripts/ci/check-release-runbook-truth.py`, "
)
_CRATES_REGISTRY_TOKEN = "CARGO_REGISTRY_TOKEN"
_CRATES_SECRET_TOKEN_SOURCE_MESSAGE = (
    "publish-crates CARGO_REGISTRY_TOKEN is sourced from secrets.*, not the auth step output"
)
_BEFORE_CLAIM = re.compile(
    r"GitHub Release is created before crates publication",
    re.IGNORECASE,
)
_REVERSE_CLAIM = re.compile(
    r"crates publication.{0,80}before.{0,80}GitHub Release",
    re.IGNORECASE | re.DOTALL,
)
_NEEDS_RELEASE_IN_DOCS = re.compile(
    r"needs `release`|needs:\s*release(?:\s|,|\]|$)",
)


def _mapping_block(text: str, key: str, indent: int) -> str:
    prefix = " " * indent + key + ":"
    lines = text.splitlines(keepends=True)
    starts = [i for i, line in enumerate(lines) if line.rstrip("\r\n") == prefix]
    if len(starts) != 1:
        raise AssertionError(
            f"expected one {key!r} mapping at indent {indent}, found {len(starts)}"
        )
    start = starts[0]
    end = start + 1
    while end < len(lines):
        raw = lines[end]
        if raw.strip() and len(raw) - len(raw.lstrip(" ")) <= indent:
            break
        end += 1
    return "".join(lines[start:end])


def _child_entries(block: str) -> dict[str, str]:
    lines = block.splitlines()
    if not lines:
        raise AssertionError("empty mapping block")
    parent_indent = len(lines[0]) - len(lines[0].lstrip(" "))
    child_indent = parent_indent + 2
    entries: dict[str, str] = {}
    for line in lines[1:]:
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        indent = len(line) - len(line.lstrip(" "))
        if indent <= parent_indent:
            break
        if indent != child_indent:
            continue
        if ":" not in line:
            continue
        key, value = line[child_indent:].split(":", 1)
        key = key.strip()
        if not key:
            continue
        if key in entries:
            raise AssertionError(f"duplicate mapping key {key!r}")
        entries[key] = value
    return entries


def _flow_or_scalar_ids(raw: str) -> set[str] | None:
    text = raw.split(" #", 1)[0].strip()
    if not text or text in {">", ">-", "|", "|-"}:
        return None
    if text.startswith("[") and text.endswith("]"):
        return {
            part.strip().strip("'\"")
            for part in text[1:-1].split(",")
            if part.strip()
        }
    return {text.strip("'\"")}


def _job_needs_ids(job: str) -> set[str]:
    lines = job.splitlines()
    if not lines:
        raise AssertionError("empty job mapping")
    job_indent = len(lines[0]) - len(lines[0].lstrip(" "))
    needs_indent = job_indent + 2
    prefix = " " * needs_indent + "needs:"
    for index, line in enumerate(lines):
        if not line.startswith(prefix):
            continue
        rest = line[len(prefix):]
        parsed = _flow_or_scalar_ids(rest)
        if parsed is not None:
            return parsed
        values: set[str] = set()
        for child in lines[index + 1:]:
            if not child.strip() or child.lstrip().startswith("#"):
                continue
            indent = len(child) - len(child.lstrip(" "))
            if indent <= needs_indent:
                break
            stripped = child.strip()
            if stripped.startswith("- "):
                values.add(stripped[2:].split(" #", 1)[0].strip().strip("'\""))
        return values
    return set()


def _has_verify_lsm_dispatch_input(workflow: str) -> bool:
    on_block = _mapping_block(workflow, "on", 0)
    dispatch = _mapping_block(on_block, "workflow_dispatch", 2)
    inputs = _mapping_block(dispatch, "inputs", 4)
    return "verify_lsm" in _child_entries(inputs)


def _publish_crates_needs_release(workflow: str) -> bool:
    jobs = _mapping_block(workflow, "jobs", 0)
    job = _mapping_block(jobs, "publish-crates", 2)
    return "release" in _job_needs_ids(job)


def _direct_step_blocks(job: str) -> list[list[str]]:
    """Return list-item step blocks under the job's direct steps: key."""
    step_blocks: list[list[str]] = []
    current: list[str] = []
    in_steps = False
    for line in job.splitlines():
        stripped = line.lstrip()
        if not stripped or stripped.startswith("#"):
            continue
        indent = len(line) - len(stripped)
        if indent == 4 and stripped == "steps:":
            in_steps = True
            continue
        if in_steps and indent <= 4:
            break
        if not in_steps:
            continue
        if indent == 6 and stripped.startswith("- "):
            if current:
                step_blocks.append(current)
            current = [line]
        elif current and indent > 6:
            current.append(line)
    if current:
        step_blocks.append(current)
    return step_blocks


def _step_direct_fields(block: list[str]) -> dict[str, str]:
    """Direct keys of a list-item step (indent 6/8), comment-stripped values."""
    fields: dict[str, str] = {}
    for line in block:
        stripped = line.lstrip()
        indent = len(line) - len(stripped)
        if indent == 6 and stripped.startswith("- "):
            direct = stripped[2:]
        elif indent == 8:
            direct = stripped
        else:
            continue
        match = re.match(r"^([^:]+?)\s*:\s*(.*)$", direct)
        if match is None:
            continue
        key = match.group(1).strip().strip("'\"")
        value = match.group(2).split(" #", 1)[0].strip(" '\"")
        if key:
            fields[key] = value
    return fields


def _step_env_entries(block: list[str]) -> dict[str, str] | None:
    """Return the step's env mapping. Empty if absent; None if unparsable."""
    text = "".join(line + "\n" for line in block)
    try:
        return _child_entries(_mapping_block(text, "env", 8))
    except AssertionError as exc:
        if "found 0" in str(exc):
            return {}
        return None


def _active_step_uses(job: str) -> list[str]:
    """Return direct-step actions that have no step-level condition."""
    uses: list[str] = []
    for block in _direct_step_blocks(job):
        fields = _step_direct_fields(block)
        if fields.get("if"):
            continue
        action = fields.get("uses", "")
        if action:
            uses.append(action)
    return uses


def _publisher_job(
    workflow: str, job_id: str, label: str
) -> tuple[str | None, dict[str, str] | None, list[str]]:
    try:
        jobs = _mapping_block(workflow, "jobs", 0)
        job = _mapping_block(jobs, job_id, 2)
        entries = _child_entries(job)
    except AssertionError as exc:
        return None, None, [f"release.yml {label} publisher identity is unavailable ({exc})"]
    return job, entries, []


def _job_environment(entries: dict[str, str]) -> str:
    return entries.get("environment", "").split(" #", 1)[0].strip(" '\"")


def _environment_problems(
    entries: dict[str, str], *, job_id: str, expected: str
) -> list[str]:
    environment = _job_environment(entries)
    if environment != expected:
        return [f"{job_id} environment is {environment!r}, expected {expected!r}"]
    return []


def _id_token_write_problems(job: str, *, job_id: str) -> list[str]:
    try:
        permissions = _child_entries(_mapping_block(job, "permissions", 4))
    except AssertionError as exc:
        return [f"{job_id} permissions are unavailable ({exc})"]
    id_token = permissions.get("id-token", "").split(" #", 1)[0].strip(" '\"")
    if id_token != "write":
        return [f"{job_id} does not grant id-token: write"]
    return []


def _pinned_publish_action_problems(
    job: str, *, job_id: str, action: str, label: str
) -> list[str]:
    if _active_step_uses(job).count(action) != 1:
        return [
            f"{job_id} does not invoke the pinned {label} trusted-publishing action exactly once"
        ]
    return []


def _crates_registry_token_source_problems(job: str) -> list[str]:
    auth_ids: list[str] = []
    token_sources: list[str] = []
    for block in _direct_step_blocks(job):
        fields = _step_direct_fields(block)
        if fields.get("if"):
            continue
        if fields.get("uses") == _CRATES_PUBLISH_ACTION:
            auth_ids.append(fields.get("id", ""))
        env_entries = _step_env_entries(block)
        if env_entries is None:
            return [
                "publish-crates cannot determine the crates.io registry token source"
            ]
        raw = env_entries.get(_CRATES_REGISTRY_TOKEN)
        if raw is None:
            continue
        token_sources.append(raw.split(" #", 1)[0].strip(" '\""))
    if len(token_sources) != 1:
        return [
            "publish-crates cannot determine the crates.io registry token source"
        ]
    source = token_sources[0]
    if re.search(r"\bsecrets\.", source):
        return [_CRATES_SECRET_TOKEN_SOURCE_MESSAGE]
    if len(auth_ids) != 1 or not auth_ids[0]:
        return ["publish-crates cannot determine the crates.io auth step id"]
    expected = "${{ steps." + auth_ids[0] + ".outputs.token }}"
    if source != expected:
        return [
            f"publish-crates CARGO_REGISTRY_TOKEN is {source!r}, expected {expected!r}"
        ]
    return []


def _pypi_workflow_problems(workflow: str) -> list[str]:
    job, entries, problems = _publisher_job(workflow, "publish-pypi", "PyPI")
    if job is None:
        return problems
    assert entries is not None
    problems.extend(
        _environment_problems(
            entries, job_id="publish-pypi", expected=_PYPI_ENVIRONMENT
        )
    )
    problems.extend(_id_token_write_problems(job, job_id="publish-pypi"))
    problems.extend(
        _pinned_publish_action_problems(
            job,
            job_id="publish-pypi",
            action=_PYPI_PUBLISH_ACTION,
            label="PyPI",
        )
    )
    return problems


def _crates_workflow_problems(workflow: str) -> list[str]:
    job, entries, problems = _publisher_job(workflow, "publish-crates", "crates.io")
    if job is None:
        return problems
    assert entries is not None
    problems.extend(
        _environment_problems(
            entries, job_id="publish-crates", expected=_CRATES_ENVIRONMENT
        )
    )
    problems.extend(_id_token_write_problems(job, job_id="publish-crates"))
    problems.extend(
        _pinned_publish_action_problems(
            job,
            job_id="publish-crates",
            action=_CRATES_PUBLISH_ACTION,
            label="crates.io",
        )
    )
    problems.extend(_crates_registry_token_source_problems(job))
    return problems


def _watch_ci(docs: str) -> str:
    start = docs.find("**Watch CI**")
    if start < 0:
        raise AssertionError("runbook is missing Watch CI")
    rest = docs[start:]
    cut = rest.find("\n### ")
    return rest if cut < 0 else rest[:cut]


def _checklist_item(docs: str, title: str) -> str:
    heading = f"**{title}**"
    idx = docs.find(heading)
    if idx < 0:
        raise AssertionError(f"runbook is missing {title} checklist item")
    line_start = docs.rfind("\n", 0, idx)
    start = 0 if line_start < 0 else line_start + 1
    prefix = docs[start:idx]
    if not re.fullmatch(r"- \[[ x]\] ", prefix):
        raise AssertionError(f"{title} is not a checklist item")
    lines = docs[start:].splitlines(keepends=True)
    collected = [lines[0]]
    for line in lines[1:]:
        if line.startswith(("- ", "#")):
            break
        collected.append(line)
    return "".join(collected)


def _optional_lsm_item(docs: str) -> str:
    return _checklist_item(docs, _LSM_ITEM_TITLE)


def _visible_docs(docs: str) -> str:
    return re.sub(r"<!--.*?-->", "", docs, flags=re.DOTALL)


def _item_sentences(item: str) -> list[str]:
    text = _CHECKLIST_ITEM_PREFIX.sub("", " ".join(item.split()), count=1)
    return [part.strip() for part in re.split(r"(?<=\.)\s+", text) if part.strip()]


def _mentions_environment(text: str) -> bool:
    return "environment" in text.lower()


def _environment_allowlist(
    required_sentences: tuple[tuple[str, str], ...],
) -> frozenset[str]:
    allowed: set[str] = set()
    for sentence, _ in required_sentences:
        if not _mentions_environment(sentence):
            continue
        if sentence.startswith("compare its expected identity"):
            first = sentence.split(". ", 1)[0].rstrip(".") + "."
            allowed.add(_RECEIPT_COMMAND_PREFIX + first)
        else:
            allowed.add(sentence)
    return frozenset(allowed)


def _unpinned_environment_sentence_problems(
    item: str,
    required_sentences: tuple[tuple[str, str], ...],
    *,
    label: str,
) -> list[str]:
    allowed = _environment_allowlist(required_sentences)
    return [
        f"{label} item includes an unpinned sentence that mentions environment"
        for sentence in _item_sentences(item)
        if _mentions_environment(sentence) and sentence not in allowed
    ]


def _trusted_publisher_docs_problems(
    docs: str,
    *,
    title: str,
    label: str,
    required_sentences: tuple[tuple[str, str], ...],
) -> list[str]:
    try:
        item = _checklist_item(_visible_docs(docs), title)
    except AssertionError as exc:
        return [str(exc)]

    problems: list[str] = []
    normalized = " ".join(item.split())
    for sentence, message in required_sentences:
        if sentence not in normalized:
            problems.append(message)
    problems.extend(
        _unpinned_environment_sentence_problems(
            item, required_sentences, label=label
        )
    )
    return problems


def _pypi_docs_problems(docs: str) -> list[str]:
    return _trusted_publisher_docs_problems(
        docs,
        title=_PYPI_ITEM_TITLE,
        label="PyPI Trusted Publisher",
        required_sentences=(
            (
                _PYPI_SINGLE_PUBLISHER_SENTENCE,
                "PyPI Trusted Publisher item does not require exactly one publisher",
            ),
            (
                _PYPI_LEGACY_REMOVAL_SENTENCE,
                "PyPI Trusted Publisher item does not require removal of the legacy publish.yml publisher",
            ),
            (
                _PYPI_EMPTY_ENVIRONMENT_SENTENCE,
                "PyPI Trusted Publisher item does not reject an empty environment",
            ),
            (
                _PYPI_RECEIPT_SENTENCE,
                "PyPI Trusted Publisher item omits the redacted owner-visible receipt",
            ),
        ),
    )


def _crates_docs_problems(docs: str) -> list[str]:
    return _trusted_publisher_docs_problems(
        docs,
        title=_CRATES_ITEM_TITLE,
        label="crates.io Trusted Publishing",
        required_sentences=(
            (
                _CRATES_IDENTITY_SENTENCE,
                "crates.io Trusted Publishing item does not require the crates.io publisher identity per crate",
            ),
            (
                _CRATES_LEGACY_REMOVAL_SENTENCE,
                "crates.io Trusted Publishing item does not require removal of publishers whose environment is unset",
            ),
            (
                _CRATES_UNSET_ENVIRONMENT_SENTENCE,
                "crates.io Trusted Publishing item does not reject an unset environment",
            ),
            (
                _CRATES_RECEIPT_SENTENCE,
                "crates.io Trusted Publishing item omits the redacted crates.io receipt sentence ending with Apply this on every current crates.io crate",
            ),
        ),
    )


def _pypi_publisher_problems(workflow: str, docs: str) -> list[str]:
    return _pypi_workflow_problems(workflow) + _pypi_docs_problems(docs)


def _crates_publisher_problems(workflow: str, docs: str) -> list[str]:
    return _crates_workflow_problems(workflow) + _crates_docs_problems(docs)


def _trusted_publisher_identity_json(
    *, environment: str, repository: str, workflow: str
) -> str:
    return json.dumps(
        {
            "environment": environment,
            "publisher_count": 1,
            "repository": repository,
            "workflow": workflow,
        },
        sort_keys=True,
        separators=(",", ":"),
    )


def _bullet_mentions_github_release(line: str) -> bool:
    stripped = line.strip()
    return stripped.startswith("-") and "`Create GitHub Release`" in stripped


def _bullet_mentions_crates(line: str) -> bool:
    stripped = line.strip()
    if not stripped.startswith("-"):
        return False
    return (
        "`publish-crates`" in stripped
        or "`Publish to crates.io`" in stripped
        or "`Publish to Crates.io`" in stripped
    )


def _crates_before_github_in_watch_ci(docs: str) -> bool:
    section = _watch_ci(docs)
    github_idx: int | None = None
    crates_idx: int | None = None
    for index, line in enumerate(section.splitlines()):
        if github_idx is None and _bullet_mentions_github_release(line):
            github_idx = index
        if crates_idx is None and _bullet_mentions_crates(line):
            crates_idx = index
    if crates_idx is None:
        return False
    return github_idx is None or crates_idx < github_idx


def _workflow_problems(workflow: str) -> list[str]:
    problems: list[str] = []
    try:
        if not _has_verify_lsm_dispatch_input(workflow):
            problems.append(
                "release.yml workflow_dispatch inputs do not declare verify_lsm"
            )
    except AssertionError as exc:
        problems.append(
            f"release.yml workflow_dispatch inputs do not declare verify_lsm ({exc})"
        )
    try:
        if not _publish_crates_needs_release(workflow):
            problems.append("publish-crates job does not need release")
    except AssertionError as exc:
        problems.append(f"publish-crates job does not need release ({exc})")
    return problems


def _lsm_item_problems(docs: str) -> list[str]:
    try:
        item = _optional_lsm_item(docs)
    except AssertionError as exc:
        return [str(exc)]
    problems: list[str] = []
    if not re.search(r"optional", item, re.IGNORECASE):
        problems.append(
            "Optional LSM verification item does not state LSM verification is optional"
        )
    if not re.search(r"not a stable-release requirement", item, re.IGNORECASE):
        problems.append(
            "Optional LSM verification item does not state LSM verification is not a stable-release requirement"
        )
    if "workflow_dispatch" not in item or "`verify_lsm`" not in item:
        problems.append(
            "Optional LSM verification item omits the workflow_dispatch verify_lsm alternative"
        )
    if _LOCAL_LSM_CMD not in item:
        problems.append(
            "Optional LSM verification item omits the local "
            "scripts/verify_lsm_docker.sh --release-tag vX.Y.Z alternative"
        )
    return problems


def _docs_problems(docs: str) -> list[str]:
    problems: list[str] = []
    if _LSM_SMOKE in docs:
        problems.append(
            "runbook names nonexistent workflow lsm-smoke-test"
        )
    problems.extend(_lsm_item_problems(docs))
    if "publish-crates" not in docs or not _NEEDS_RELEASE_IN_DOCS.search(docs):
        problems.append(
            "runbook omits that publish-crates needs release"
        )
    if not _BEFORE_CLAIM.search(docs):
        problems.append(
            "runbook does not state that the GitHub Release is created before crates publication"
        )
    try:
        reverse_bullets = _crates_before_github_in_watch_ci(docs)
    except AssertionError as exc:
        problems.append(str(exc))
        reverse_bullets = False
    if reverse_bullets or _REVERSE_CLAIM.search(docs):
        problems.append(
            "runbook documents crates publication before GitHub Release"
        )
    return problems


def contract_problems(workflow: str, docs: str) -> list[str]:
    return (
        _workflow_problems(workflow)
        + _docs_problems(docs)
        + _pypi_publisher_problems(workflow, docs)
        + _crates_publisher_problems(workflow, docs)
    )


def main() -> int:
    if len(sys.argv) != 1:
        print("FAIL: this checker accepts no arguments", file=sys.stderr)
        return 2
    problems = contract_problems(
        WORKFLOW.read_text(encoding="utf-8"),
        DOCS.read_text(encoding="utf-8"),
    )
    if problems:
        for problem in problems:
            print(f"FAIL: {problem}", file=sys.stderr)
        return 1
    print("ok   release runbook matches executable release.yml")
    print(
        "expected_pypi_trusted_publisher="
        + _trusted_publisher_identity_json(
            environment=_PYPI_ENVIRONMENT,
            repository=_PYPI_REPOSITORY,
            workflow=_PYPI_WORKFLOW,
        )
    )
    print(
        "expected_crates_trusted_publisher="
        + _trusted_publisher_identity_json(
            environment=_CRATES_ENVIRONMENT,
            repository=_CRATES_REPOSITORY,
            workflow=_CRATES_WORKFLOW,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
