#!/usr/bin/env python3
"""Docs/workflow contract: release runbook may name only executable release.yml surfaces."""
from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
WORKFLOW = REPO / ".github/workflows/release.yml"
DOCS = REPO / "docs/reference/release.md"

_LSM_SMOKE = "lsm-smoke-test"
_LSM_ITEM_TITLE = "Optional LSM verification"
_LOCAL_LSM_CMD = "scripts/verify_lsm_docker.sh --release-tag vX.Y.Z"
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
        if line.startswith("- ") or line.startswith("#"):
            break
        collected.append(line)
    return "".join(collected)


def _optional_lsm_item(docs: str) -> str:
    return _checklist_item(docs, _LSM_ITEM_TITLE)


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
    return _workflow_problems(workflow) + _docs_problems(docs)


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
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
