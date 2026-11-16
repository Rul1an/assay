#!/usr/bin/env python3
"""Count the calls that read a document outside the shared reader (ADR-052, decision 5).

The entry points of the YAML crate, and `serde_ignored`'s, are listed in
`scripts/ci/closed-reader/clippy.toml` under the crate's real name. This script runs clippy over
the workspace's library and binary targets with that list and `--force-warn`, so every call is
reported whatever `#[allow]` sits on it, and compares the calls per file with the ledger in
`scripts/ci/closed-reader/ledger.tsv`.

It fails when a file has more calls than its ledger line, when a file with a call has no ledger
line, and when a ledger line names more calls than were found. The last one keeps the ledger equal
to the code: a pull request that removes a call lowers the line in the same change, and a run that
finds nothing at all (a list that matches no path, a lint that did not run) is a failure and not a
clean result. The shared reader's own calls are in the ledger, so every run has to find them.

What is not counted: test code (the targets are `--lib --bins`, where `cfg(test)` is off), and
code behind a feature that is not on by default. The shared reader is such code until a reader
uses it outside tests; from then on its own calls are in the ledger like any other.

Every run lints the workspace again. cargo replays a crate's earlier diagnostics when it judges
the crate unchanged, and it judges by modification time: a file restored with an older time is
"unchanged", and the replay is of whatever list and code the earlier run saw. A value that differs
on every run is passed to the lint driver, which cargo tracks, so nothing is replayed.

Exit status: 0 the ledger and the code agree; 1 they do not; 2 clippy did not finish, so nothing
was counted.

A file that is only compiled on Linux has `linux` as a third column. Off Linux that line is
skipped, and the run says so. Every other line is exact on every platform.
"""

from __future__ import annotations

import argparse
import json
import os
import secrets
import subprocess
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CONF_DIR = ROOT / "scripts" / "ci" / "closed-reader"
LEDGER = CONF_DIR / "ledger.tsv"
LINT = "clippy::disallowed_methods"
SHARED_READER = "crates/assay-common/src/doc.rs"


def read_ledger(text: str, on_linux: bool) -> tuple[dict[str, int], list[str]]:
    """`count<TAB>path[<TAB>linux]` per line; `#` starts a comment; a path appears once.

    Returns the lines that apply on this platform, and the paths skipped because they are
    marked `linux` and this is not Linux.
    """
    ledger: dict[str, int] = {}
    skipped: list[str] = []
    for number, raw in enumerate(text.splitlines(), 1):
        line = raw.split("#", 1)[0].rstrip()
        if not line:
            continue
        parts = line.split("\t")
        shaped = len(parts) in (2, 3) and parts[0].isdigit() and int(parts[0]) >= 1
        if not shaped or (len(parts) == 3 and parts[2] != "linux"):
            raise SystemExit(f"ledger line {number}: expected `count<TAB>path` or `count<TAB>path<TAB>linux`")
        if parts[1] in ledger or parts[1] in skipped:
            raise SystemExit(f"ledger line {number}: `{parts[1]}` is listed twice")
        if len(parts) == 3 and not on_linux:
            skipped.append(parts[1])
        else:
            ledger[parts[1]] = int(parts[0])
    return ledger, skipped


def calls_from(stream) -> Counter[str]:
    """Calls per file from cargo's JSON messages, each source position counted once."""
    seen: set[tuple[str, int, int]] = set()
    for raw in stream:
        raw = raw.strip()
        if not raw.startswith("{"):
            continue
        try:
            event = json.loads(raw)
        except json.JSONDecodeError:
            continue
        if event.get("reason") != "compiler-message":
            continue
        message = event.get("message") or {}
        if (message.get("code") or {}).get("code") != LINT:
            continue
        for span in message.get("spans") or []:
            if span.get("is_primary"):
                seen.add((span["file_name"], span["line_start"], span["column_start"]))
    return Counter(name for name, _, _ in seen)


def run_clippy() -> Counter[str]:
    env = dict(os.environ, CLIPPY_CONF_DIR=str(CONF_DIR))
    # The cfg is read by no code. Its value is new on every run, which is what makes cargo lint
    # every workspace crate again instead of replaying an earlier run.
    fresh = f'closed_reader_count_run="{secrets.token_hex(8)}"'
    command = [
        "cargo", "clippy", "--locked", "--workspace", "--lib", "--bins",
        "--message-format=json", "--", "--force-warn", LINT, "--cfg", fresh,
    ]
    done = subprocess.run(command, cwd=ROOT, env=env, stdout=subprocess.PIPE, text=True, check=False)
    if done.returncode != 0:
        print(f"closed-reader-count: clippy exited {done.returncode}; nothing was counted", file=sys.stderr)
        raise SystemExit(2)
    return calls_from(done.stdout.splitlines())


def compare(found: Counter[str], ledger: dict[str, int]) -> list[str]:
    """Every reason the run fails, in a fixed order."""
    problems: list[str] = []
    for name in sorted(found):
        if name not in ledger:
            problems.append(f"new: {name} has {found[name]} call(s) and no ledger line")
        elif found[name] > ledger[name]:
            problems.append(f"rose: {name} has {found[name]} call(s), the ledger says {ledger[name]}")
    for name in sorted(ledger):
        if found.get(name, 0) < ledger[name]:
            problems.append(
                f"fell: {name} has {found.get(name, 0)} call(s), the ledger says {ledger[name]}; "
                "lower the ledger line in this change"
            )
    return problems


def report(found: Counter[str], ledger: dict[str, int], skipped: list[str]) -> int:
    problems = compare(found, ledger)
    total = sum(found.values())
    outside = total - found.get(SHARED_READER, 0)
    print(f"closed-reader-count: {total} call(s) in {len(found)} file(s); {outside} outside the shared reader")
    for name in skipped:
        print(f"closed-reader-count: skipped {name}: its ledger line is marked linux and this is {sys.platform}")
    for problem in problems:
        print(f"closed-reader-count: FAIL {problem}")
    return 1 if problems else 0


def self_test() -> int:
    def event(name: str, line: int, code: str = LINT, primary: bool = True) -> str:
        return json.dumps({
            "reason": "compiler-message",
            "message": {
                "code": {"code": code},
                "spans": [{"file_name": name, "line_start": line, "column_start": 5, "is_primary": primary}],
            },
        })

    base = [event("a.rs", 1), event("a.rs", 9), event(SHARED_READER, 3)]
    book = f"# a comment\n2\ta.rs\n1\t{SHARED_READER}\n"
    with_linux = book + "1\tl.rs\tlinux\n"
    cases = [
        ("the ledger and the code agree", base, book, True, []),
        ("one position reported by two targets counts once", base + [event("a.rs", 1)], book, True, []),
        ("another lint is not a call", base + [event("a.rs", 20, code="clippy::todo")], book, True, []),
        ("a secondary span is not a call", base + [event("a.rs", 21, primary=False)], book, True, []),
        ("a build line that is not JSON is skipped", base + ["   Compiling x v1"], book, True, []),
        ("one more call in a listed file", base + [event("a.rs", 30)], book, True, ["rose"]),
        ("a call in a file with no line", base + [event("b.rs", 1)], book, True, ["new"]),
        ("a call removed and the ledger not lowered", base[1:], book, True, ["fell"]),
        ("the same off Linux", base[1:], book, False, ["fell"]),
        ("nothing found at all", [], book, True, ["fell", "fell"]),
        ("nothing found at all, off Linux", [], book, False, ["fell", "fell"]),
        ("a linux line is exact on Linux", base, with_linux, True, ["fell"]),
        ("a linux line is skipped off Linux", base, with_linux, False, []),
        ("a call in a linux file off Linux has no line there", base + [event("l.rs", 1)], with_linux, False, ["new"]),
    ]
    failed = 0
    for name, events, text, on_linux, expected in cases:
        ledger, _ = read_ledger(text, on_linux)
        got = [problem.split(":", 1)[0] for problem in compare(calls_from(events), ledger)]
        if got != expected:
            failed += 1
            print(f"self-test FAIL: {name}: expected {expected}, got {got}")
    for name, text in [
        ("a count of zero", "0\ta.rs\n"),
        ("a path listed twice", "1\ta.rs\n2\ta.rs\n"),
        ("a third column that is not linux", "1\ta.rs\tmacos\n"),
        ("spaces instead of a tab", "1 a.rs\n"),
    ]:
        try:
            read_ledger(text, True)
        except SystemExit:
            continue
        failed += 1
        print(f"self-test FAIL: {name}: the ledger was accepted")
    total = len(cases) + 4
    print(f"closed-reader-count self-test: {total - failed}/{total} cases")
    return 1 if failed else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n", 1)[0])
    parser.add_argument("--self-test", action="store_true", help="check the comparison on fixed inputs")
    parser.add_argument("--from-json", type=Path, help="read cargo's JSON messages from a file instead of running clippy")
    parser.add_argument("--list", action="store_true", help="print the calls per file as ledger lines and exit")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if args.from_json:
        found = calls_from(args.from_json.read_text(encoding="utf-8").splitlines())
    else:
        found = run_clippy()
    if args.list:
        for name in sorted(found):
            print(f"{found[name]}\t{name}")
        return 0
    ledger, skipped = read_ledger(LEDGER.read_text(encoding="utf-8"), sys.platform.startswith("linux"))
    return report(found, ledger, skipped)


if __name__ == "__main__":
    sys.exit(main())
