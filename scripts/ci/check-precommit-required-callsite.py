#!/usr/bin/env python3
"""Assert every pre-commit guard that judges a required workflow can fail a merge.

`scripts/ci/test-ci-gate-expectations.sh` stated a contract about `ci.yml`'s merge gate and
reached CI only through `pre-commit run --all-files` in kernel-matrix.yml's `Lint (pre-commit)`
job, which is not among the required contexts. It could not fail a merge (#2878).
`scripts/ci/test-ci-job-timeouts-contract.sh` was the same defect (#2879). Both were fixed one
at a time; a measurement over all 86 hooks afterwards found seven more. Fixing an eighth by
hand is the wrong move -- nothing was stopping a ninth from being written tomorrow.

So the rule is derived, not restated. A hook is in scope when one of the scripts in its `entry:`
binds a required workflow path outside a comment -- `WORKFLOW="${ROOT}/.github/workflows/ci.yml"`
is the shape all nine had. That script, or another of the hook's scripts, must then appear as an
active line in some required workflow. A hook added tomorrow is covered by construction, because
it starts with neither a callsite nor a marker.

The required set is derived too, from the checked-in ruleset, along the two ways this repo
produces a required context:

  * a job whose display name is a required context (`CI`, `host-capability-check`, ...); and
  * a script that declares `STATUS_CONTEXT = "<context>"` and is run by some workflow, which is
    how `lane-check/proof` is posted -- it is a commit status, not a job.

A context that resolves to no workflow is an error, not a smaller required set: that is how a
renamed job or a moved status producer surfaces, rather than quietly shrinking what this guard
protects. Anything this cannot parse is an error for the same reason.

A hook's scripts are the files under `scripts/` that its `entry:` names, whatever the language. An
earlier version recognised only `.sh` and `.py`, so a guard written in `.mjs` was never in scope.
Directories are dropped: the path of every script under `scripts/ci` contains `scripts/ci`, so a
directory token would match any callsite of any of them and read as wired for any hook that named
it. Whatever the language, the text after a `#` is what gets stripped. So a path in a JavaScript
`//` comment or a Python docstring counts as a binding, the deliberate direction described below;
and a `#` that is not a comment -- a JavaScript private field, a URL fragment -- cuts its line
short, which would hide a binding later on that line. The one such line among the hooks' scripts
is this guard's own negative control, where it is the point.

A callsite is an uncommented line of a required workflow that contains the script's path. That
is presence, not execution: a path in an `env:` value or an `echo` would count. The hardening
step's closed command set pins execution for the commands listed there; for any other callsite,
this guard does not.

Opting out is possible and visible. A hook may carry

    # required-callsite: local-only -- <reason>

indented into its block, at the level of the hook's keys. A marker belongs to the block it is
indented into, not to the last hook seen: attributing it that way made a marker written above
the next hook -- the usual place for a comment about that hook -- exempt the one before it. A
marker in no block exempts nothing and is reported. Indentation decides, which leaves one edge:
a marker at key level after a hook's last key, separated from the next hook only by blank or
comment lines, still belongs to the hook above it. That is where YAML puts it, but a writer who
meant it for the next hook is not warned. The reason floor is not about prose: it is there so
the marker cannot be added as reflex punctuation while a reviewer skims past it.

This guard is in its own scope and passes the way every other hook does, by having a callsite.
An earlier version of this paragraph claimed the opposite -- that it built paths from a constant
and so could not match itself. That was wrong, and replaying the guard against a tree where its
own ci.yml lines were absent falsified it: the guard reported itself, correctly. `uncommented()`
strips `#` comments only, so the workflow path in this module's docstring above and in the
self-test's positive control both count, and `binding_names` returns {"ci.yml"} for this file.

That over-inclusion is deliberate in direction rather than accidental. A path named in a Python
docstring or a string literal is prose, and treating it as a binding asks for a callsite that
may not be needed -- the error falls toward wiring a contract into CI rather than away from it,
and the marker is the escape when it lands wrongly. The rule reads cleanly for shell scripts,
where `#` is the only comment form; for Python it is broader than "asserts against".

The callsite is also pinned, independently of this rule, by the "Verify CI hardening contracts"
step's closed command set, restated in three places: dropping the line turns all three red.

Usage:
    check-precommit-required-callsite.py             # verify every in-scope hook is wired
    check-precommit-required-callsite.py --self-test # prove the guard detects each defeat
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]


def _gate_coverage():
    """The ruleset is read by one function, shared with check-ci-gate-coverage.py.

    Two readings of "which contexts are required" would be two answers to the same question,
    and the drift would be invisible from either side.
    """
    import importlib.util

    path = REPO_ROOT / "scripts/ci/check-ci-gate-coverage.py"
    spec = importlib.util.spec_from_file_location("check_ci_gate_coverage", path)
    if spec is None or spec.loader is None:
        raise CallsiteError(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

CONFIG = Path(".pre-commit-config.yaml")
RULESET = Path(".github/rulesets/main-required-ci-contexts.json")
WORKFLOW_DIR = Path(".github/workflows")

MARKER_PREFIX = "# required-callsite: local-only --"
MIN_REASON_CHARS = 30

HOOK_ID_RE = re.compile(r"^      - id: (?P<id>\S+)\s*$")
HOOK_ENTRY_RE = re.compile(r"^        entry: (?P<entry>.*)$")
# A hook's keys sit at this indentation. A less-indented line that is neither blank nor a
# comment ends the block -- the next `- id:`, or the next `- repo:` and its `hooks:`. A marker
# must itself sit at this level to count.
HOOK_KEY_INDENT = 8
MARKER_RE = re.compile(rf"^(?P<indent>\s*){re.escape(MARKER_PREFIX)}\s*(?P<reason>\S.*)$")
# Any path under scripts/, whatever its extension; `hook_scripts` keeps the ones that are files.
# A closed extension list skipped the `.mjs` guard, and finding "the word after the interpreter"
# fails on `node --test --test-reporter spec <path>` and on `f=<path>; python3 $f`.
SCRIPT_RE = re.compile(r"scripts/[A-Za-z0-9_./-]+")
JOB_KEY_RE = re.compile(r"^  (?P<name>[A-Za-z0-9_][A-Za-z0-9_-]*):\s*(?:#.*)?$")
JOB_NAME_RE = re.compile(r"^    name:\s*(?P<value>.+?)\s*$")
STATUS_CONTEXT_RE = re.compile(r"^STATUS_CONTEXT\s*=\s*[\"'](?P<context>[^\"']+)[\"']", re.M)


class CallsiteError(Exception):
    """The config, the workflows or the ruleset could not be read the way this guard needs."""


def read_repo(path: Path) -> str:
    target = REPO_ROOT / path
    if not target.is_file():
        raise CallsiteError(f"{path}: missing")
    return target.read_text(encoding="utf-8")


def uncommented(text: str) -> list[str]:
    """Lines with their comment tail removed, so a mention in prose is not a binding."""
    return [line.split("#", 1)[0] for line in text.splitlines()]


def workflow_texts() -> dict[str, str]:
    directory = REPO_ROOT / WORKFLOW_DIR
    if not directory.is_dir():
        raise CallsiteError(f"{WORKFLOW_DIR}: missing")
    found = {p.name: p.read_text(encoding="utf-8") for p in sorted(directory.glob("*.yml"))}
    if not found:
        raise CallsiteError(f"{WORKFLOW_DIR}: no workflows to read")
    return found


def job_display_names(text: str) -> set[str]:
    lines = text.splitlines()
    names: set[str] = set()
    keys = [i for i, line in enumerate(lines) if JOB_KEY_RE.match(line)]
    for position, index in enumerate(keys):
        stop = keys[position + 1] if position + 1 < len(keys) else len(lines)
        job_id = JOB_KEY_RE.match(lines[index])["name"]
        names.add(job_id)
        for line in lines[index:stop]:
            match = JOB_NAME_RE.match(line)
            if match:
                names.add(match["value"].strip("\"'"))
                break
    return names


def status_context_producers() -> dict[str, str]:
    """Scripts that declare a commit-status context, mapped context -> script path."""
    produced = {}
    for script in sorted((REPO_ROOT / "scripts").rglob("*.py")):
        match = STATUS_CONTEXT_RE.search(script.read_text(encoding="utf-8", errors="replace"))
        if match:
            produced[match["context"]] = script.relative_to(REPO_ROOT).as_posix()
    return produced


def required_workflows(ruleset_text: str, workflows: dict[str, str]) -> dict[str, str]:
    """Workflow file name -> the required context it produces. Every context must resolve."""
    contexts = _gate_coverage().required_contexts(ruleset_text)
    producers = status_context_producers()
    resolved: dict[str, str] = {}
    unresolved = []
    for context in sorted(contexts):
        hits = [name for name, text in workflows.items() if context in job_display_names(text)]
        if not hits and context in producers:
            script = producers[context]
            hits = [name for name, text in workflows.items() if script in text]
        if not hits:
            unresolved.append(context)
            continue
        for name in hits:
            resolved[name] = context
    if unresolved:
        raise CallsiteError(
            f"{RULESET}: required context(s) {unresolved} resolve to no workflow. Either a job "
            "or a status producer was renamed, which detaches branch protection, or the ruleset "
            "no longer names what this repo reports."
        )
    return resolved


def parse_hooks(config_text: str) -> tuple[list[dict], list[int]]:
    """The hooks, and the line numbers of opt-out markers that sit in no hook's block.

    A marker belongs to the block it is indented into: it must sit at the hook's key level,
    inside a block that is still open. Attributing it to "the last hook seen" instead made a
    marker written above the next hook -- the usual place for a comment about that hook --
    exempt the one before it, silently. An orphan is returned rather than dropped: a marker
    that exempts nothing is a mistake its writer needs to hear about.

    Only structure ends a block: a less-indented line that is not a comment. A comment is not
    structure, and letting one end the block would leave any `entry:` after it unread, so the
    hook would be skipped rather than judged.
    """
    hooks: list[dict] = []
    orphans: list[int] = []
    current: dict | None = None
    for number, line in enumerate(config_text.splitlines(), start=1):
        match = HOOK_ID_RE.match(line)
        if match:
            current = {"id": match["id"], "line": number, "entry": None, "reason": None}
            hooks.append(current)
            continue
        indent = len(line) - len(line.lstrip())
        marker = MARKER_RE.match(line)
        if marker:
            if current is None or indent < HOOK_KEY_INDENT:
                orphans.append(number)
            else:
                current["reason"] = marker["reason"].strip()
            continue
        text = line.strip()
        if text and not text.startswith("#") and indent < HOOK_KEY_INDENT:
            current = None
        if current is None:
            continue
        entry = HOOK_ENTRY_RE.match(line)
        if entry:
            current["entry"] = entry["entry"]
    if not hooks:
        raise CallsiteError(f"{CONFIG}: no hooks this guard can recognise")
    return hooks, orphans


def hook_scripts(entry: str) -> list[str]:
    """The files under scripts/ that a hook's `entry:` names. A directory is not a script."""
    return sorted({t for t in SCRIPT_RE.findall(entry) if (REPO_ROOT / t).is_file()})


def binding_names(body: str, required: set[str]) -> set[str]:
    """Which required workflows this source *binds*, as opposed to merely mentioning.

    A path inside a `#` comment is prose about a workflow, not a claim on it, and pulling such
    a script into scope would make the guard demand a callsite for a contract it does not
    state. Only `#` comments are stripped: a path in a Python docstring or a string literal
    still counts, so for `.py` sources this is broader than "asserts against". See the module
    docstring -- the direction of that error is deliberate, and the marker is the escape.
    """
    bound = set()
    for line in uncommented(body):
        for name in required:
            if f"{WORKFLOW_DIR.as_posix()}/{name}" in line:
                bound.add(name)
    return bound


def binds_required_workflow(script: str, required: set[str]) -> set[str]:
    target = REPO_ROOT / script
    if not target.is_file():
        return set()
    return binding_names(target.read_text(encoding="utf-8", errors="replace"), required)


def has_active_callsite(scripts: list[str], workflows: dict[str, str], required: set[str]) -> bool:
    for name in required:
        for line in uncommented(workflows[name]):
            if any(script in line for script in scripts):
                return True
    return False


def check(config_text: str, ruleset_text: str, workflows: dict[str, str]) -> list[str]:
    required = set(required_workflows(ruleset_text, workflows))
    hooks, orphans = parse_hooks(config_text)
    problems = [
        f"{CONFIG}:{number}: `{MARKER_PREFIX}` sits outside any hook block, so it exempts "
        f"nothing. Indent it into the block of the hook it is for, at the level of its keys."
        for number in orphans
    ]
    for hook in hooks:
        if not hook["entry"]:
            continue
        scripts = hook_scripts(hook["entry"])
        if not scripts:
            continue
        subjects = sorted({n for s in scripts for n in binds_required_workflow(s, required)})
        if not subjects:
            continue
        if has_active_callsite(scripts, workflows, required):
            continue
        reason = hook["reason"]
        if reason and len(reason) >= MIN_REASON_CHARS:
            continue
        if reason:
            problems.append(
                f"{CONFIG}:{hook['line']}: hook `{hook['id']}` judges {subjects} and opts out "
                f"with a {len(reason)}-character reason; {MIN_REASON_CHARS} is the floor"
            )
            continue
        problems.append(
            f"{CONFIG}:{hook['line']}: hook `{hook['id']}` states a contract about {subjects}, "
            f"which branch protection requires, but none of {scripts} runs from a required "
            f"workflow. `pre-commit run --all-files` reaches only the Lint job, which is not "
            f"required, so this contract cannot fail a merge. Wire it into a required workflow, "
            f"or mark it `{MARKER_PREFIX} <reason>` (#2878, #2879)."
        )
    return problems


def run_check() -> int:
    problems = check(read_repo(CONFIG), read_repo(RULESET), workflow_texts())
    for problem in problems:
        print(problem, file=sys.stderr)
    if problems:
        print(f"FAIL: {len(problems)} pre-commit contract(s) cannot fail a merge", file=sys.stderr)
        return 1
    print("PASS: every pre-commit guard that judges a required workflow runs from one")
    return 0


def self_test() -> int:
    """Each case is a way the guard could be defeated. A green guard under any of them is a bug.

    The synthetic cases use a config holding exactly one hook, so a case that goes red proves
    the guard judged *that* hook. Appending to the live config instead would let an unrelated
    hook satisfy the assertion, which is a case that passes without testing anything.
    """
    ruleset = read_repo(RULESET)
    workflows = workflow_texts()
    live_config = read_repo(CONFIG)
    failures = 0

    def synthetic(*extra: str, hook_id: str = "invented-unwired-guard",
                  entry: str = "bash scripts/ci/test-ci-gate-expectations.sh") -> str:
        body = [
            "repos:",
            "  - repo: local",
            "    hooks:",
            f"      - id: {hook_id}",
            "        name: invented",
            f"        entry: {entry}",
            "        language: system",
        ]
        body.extend(extra)
        return "\n".join(body) + "\n"

    def expect(label: str, config: str, wfs: dict[str, str],
               want: str | tuple[str, ...] | None) -> None:
        """`want` is what the guard must say: None for silence, otherwise one substring per
        problem it must report, and no others -- so a case that expects two problems fails if
        the guard reports only the one it happened to get right."""
        nonlocal failures
        try:
            problems = check(config, ruleset, wfs)
        except CallsiteError as exc:
            problems = [str(exc)]
        if want is None:
            ok = not problems
        else:
            wants = (want,) if isinstance(want, str) else want
            unmatched = list(problems)
            ok = len(problems) == len(wants)
            for needle in wants:
                hit = next((p for p in unmatched if needle in p), None)
                if hit is None:
                    ok = False
                else:
                    unmatched.remove(hit)
        if ok:
            print(f"ok    {label}")
        else:
            print(f"FAIL  {label}: problems={problems}", file=sys.stderr)
            failures += 1

    expect("the live tree is wired", live_config, workflows, None)

    # ci.yml with the callsite gone: the guard must then object to the hook that judges it.
    stripped = dict(workflows)
    stripped["ci.yml"] = workflows["ci.yml"].replace(
        "          bash scripts/ci/test-ci-gate-expectations.sh\n", "", 1)

    expect("an unwired guard is caught", synthetic(), stripped, "invented-unwired-guard")

    # A callsite that is only a comment is documentation, not an executed gate.
    commented = dict(workflows)
    commented["ci.yml"] = workflows["ci.yml"].replace(
        "          bash scripts/ci/test-ci-gate-expectations.sh",
        "          # bash scripts/ci/test-ci-gate-expectations.sh", 1)
    expect("a commented-out callsite is caught", synthetic(), commented,
           "invented-unwired-guard")

    # The marker is an escape only when a human typed a real reason.
    expect("a short opt-out reason is rejected",
           synthetic(f"        {MARKER_PREFIX} too short"), stripped,
           "invented-unwired-guard")
    expect("a real opt-out reason is accepted",
           synthetic(f"        {MARKER_PREFIX} local feedback only; the runner enforces "
                     "this contract on the head that merges"), stripped, None)

    # A marker belongs to the block it is indented into. Written above the next hook -- where a
    # comment about that hook usually goes -- it used to exempt the hook *before* it, which is
    # the unwired one here, and exempt nothing it was meant for. It must now exempt neither and
    # be reported, so the writer learns the marker did nothing.
    reason = "local feedback only; the runner enforces this contract on the head that merges"
    benign = ["      - id: invented-benign", "        name: benign",
              "        entry: bash -c true", "        language: system"]
    expect("a marker above the next hook exempts neither",
           synthetic("", f"      {MARKER_PREFIX} {reason}", *benign), stripped,
           ("invented-unwired-guard", "outside any hook block"))
    expect("a marker at repository indentation exempts nothing",
           synthetic(f"{MARKER_PREFIX} {reason}", *benign), stripped,
           ("invented-unwired-guard", "outside any hook block"))

    # A comment is not structure. One written between a hook's keys at list indentation must
    # not end the block, or the `entry:` after it is never read and the hook is skipped -- the
    # guard would pass a hook it never looked at.
    comment_between_keys = "\n".join([
        "repos:", "  - repo: local", "    hooks:",
        "      - id: invented-unwired-guard",
        "        name: invented",
        "      # a comment at list indentation, between this hook's keys",
        "        entry: bash scripts/ci/test-ci-gate-expectations.sh",
        "        language: system",
    ]) + "\n"
    expect("a comment between a hook's keys does not end its block",
           comment_between_keys, stripped, "invented-unwired-guard")

    # What does end a block is structure: the next `- repo:` and its `hooks:`. A marker at key
    # indentation after them belongs to no hook yet, so it must not reach back to the last one.
    expect("a marker after the next repo entry exempts nothing",
           synthetic("  - repo: local", "    hooks:", f"        {MARKER_PREFIX} {reason}",
                     *benign), stripped,
           ("invented-unwired-guard", "outside any hook block"))

    # A guard is in scope whatever language it is written in. `node --test` puts a flag with a
    # value before the path, so this also pins that scripts are not found by looking for the
    # word after an interpreter. The fixture is a real script that binds ci.yml.
    mjs = "scripts/ci/test_codex_host_proof.mjs"
    mjs_unwired = dict(workflows)
    mjs_unwired["ci.yml"] = workflows["ci.yml"].replace(
        f"          node --test --test-reporter spec {mjs}\n", "", 1)
    expect("a guard written in .mjs is in scope",
           synthetic(hook_id="invented-mjs-guard",
                     entry=f"node --test --test-reporter spec {mjs}"),
           mjs_unwired, "invented-mjs-guard")

    # Finding paths without an extension list also finds directories. `scripts/ci` is a
    # substring of nearly every line in ci.yml, so a directory token that survived into the
    # callsite search would wire every hook that names one. Only files are scripts.
    expect("a directory in an entry is not a callsite",
           synthetic(entry="bash -c 'python3 -m unittest discover -s scripts/ci && "
                           "bash scripts/ci/test-ci-gate-expectations.sh'"),
           stripped, "invented-unwired-guard")

    # A mention in prose is not a binding. Exercised directly rather than through a hook: no
    # script in the tree mentions a required workflow only in a comment, so a fixture routed
    # through the config would assert nothing. Both halves are checked, so a `binding_names`
    # that always returned the empty set would fail the positive control rather than pass here.
    names = {"ci.yml"}
    if binding_names('WORKFLOW="${ROOT}/.github/workflows/ci.yml"', names) != names:
        print("FAIL  a bound workflow path is recognised", file=sys.stderr)
        failures += 1
    else:
        print("ok    a bound workflow path is recognised")
    if binding_names("# see .github/workflows/ci.yml for the gate", names):
        print("FAIL  a workflow named only in a comment is not a binding", file=sys.stderr)
        failures += 1
    else:
        print("ok    a workflow named only in a comment is not a binding")

    # This guard is in its own scope. Pinned because the module docstring once claimed the
    # opposite: with its own callsite gone, the live config must name this guard's hook.
    self_unwired = dict(workflows)
    for line in ("          python3 scripts/ci/check-precommit-required-callsite.py --self-test\n",
                 "          python3 scripts/ci/check-precommit-required-callsite.py\n"):
        self_unwired["ci.yml"] = self_unwired["ci.yml"].replace(line, "", 1)
    expect("the guard is in its own scope", live_config, self_unwired,
           "precommit-required-callsite")

    # A required context nothing produces must be an error, never a smaller required set.
    renamed = dict(workflows)
    renamed["ci.yml"] = workflows["ci.yml"].replace("\nname: CI\n", "\nname: CI-renamed\n", 1)
    renamed["ci.yml"] = renamed["ci.yml"].replace("    name: CI\n", "    name: CI-renamed\n", 1)
    expect("a required context that resolves to nothing is an error",
           live_config, renamed, "resolve to no workflow")

    if failures:
        print(f"FAIL: {failures} self-test case(s)", file=sys.stderr)
        return 1
    print("PASS: pre-commit required-callsite guard self-test")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true",
                       help="prove the guard detects each way it can be defeated")
    args = parser.parse_args()
    try:
        return self_test() if args.self_test else run_check()
    except CallsiteError as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
