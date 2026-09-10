#!/usr/bin/env python3
"""Closing-keyword landing guard (#2880).

GitHub closes an issue when a closing keyword followed by an issue reference appears in a pull
request description, or in a commit message that lands on the default branch. Two properties
of that rule have closed issues on this repository that their authors meant to keep open:

- GitHub has no notion of negation. A sentence that says a change does not close an issue,
  with the issue number right after the keyword, closes that issue.
- Commit messages are frozen. Editing the PR body to retract a keyword does not remove the
  same keyword from a commit message, and the commit closes the issue when it lands.

The guard treats the PR body as the live declaration of what a merge closes, and refuses to
land when:

1. a closing keyword and issue reference sit in a negated clause, in any text GitHub parses; or
2. a commit message, or the title, closes an issue that the body does not also close with a
   non-negated keyword. The title is not parsed while it sits on the PR, but it becomes the
   subject of the merge or squash commit, which is.

Comparing against GitHub's own `closingIssuesReferences` would be circular for the first rule:
GitHub computes that set by the same parse, so a negated phrase is already in it.

The rule is deliberately conservative. It prefers a false block, which costs one rewording,
to a silent close. The remedy for a block is always the same: say what you mean without a
closing keyword next to an issue number, for example "Refs #N".
"""
from __future__ import annotations

import re

_KEYWORD = r"(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)"
_REF = (
    r"(?:(?P<repo>[A-Za-z0-9][\w.-]*/[\w.-]+)?#(?P<num>\d+)"
    r"|https://github\.com/(?P<urepo>[\w.-]+/[\w.-]+)/issues/(?P<unum>\d+))"
)
# The lookarounds give the keyword whole-word boundaries without treating a hyphen as one, so
# "disclose", "prefix", "unresolved", "fixture" and "closet" never match.
_CLOSING = re.compile(rf"(?<![\w-]){_KEYWORD}(?![\w-])\s*:?\s*{_REF}", re.IGNORECASE)

# A clause ends at sentence punctuation followed by whitespace, so "ci.yml" is not a boundary,
# and at a paragraph break. It also ends where a line starts a new structural element: a list
# item, heading, table row or blockquote. A plain soft line break does not end it, because this
# repository hard-wraps PR bodies and commit messages, so "It does not" at the end of one line and
# the keyword at the start of the next is an ordinary shape, not an edge case.
_CLAUSE_BOUNDARY = re.compile(
    r"[.!?;](?=\s)"
    r"|\n[ \t]*\n"
    r"|\n(?=[ \t]*(?:[-*+][ \t]|\d+[.)][ \t]|#{1,6}[ \t]|\||>))"
)
# Bare "no" is left out on purpose: "a no-op that closes #5" is a real close, not a negation.
# "no longer" is in, because it negates what follows it. Contractions are matched with and without
# the apostrophe, since "wont" and "doesnt" are what fast typing produces.
_NEGATORS = (
    "not", "never", "nor", "neither", "without", "cannot", "unable",
    "hardly", "barely", "scarcely", "rarely", "seldom", "aint",
    "dont", "doesnt", "didnt", "wont", "isnt", "arent", "cant", "shouldnt", "wouldnt", "couldnt",
)
_NEGATION = re.compile(
    r"(?<![\w-])(?:" + "|".join(_NEGATORS) + r"|no[ \t\n]+longer)(?![\w-])|n't(?![\w-])",
    re.IGNORECASE,
)
# Typographic apostrophes become ASCII before the negation scan: an editor or a phone turns
# "doesn't" into "doesn\u2019t", and GitHub still reads the keyword that follows.
_APOSTROPHES = str.maketrans({"\u2019": "'", "\u2018": "'", "\u02bc": "'", "\uff07": "'"})

_LINE_LIMIT = 200


def _normalise(repo: str, match: re.Match) -> tuple[tuple[str, int], str]:
    """(comparison key, display form) for one closing reference."""
    ref_repo = match.group("repo") or match.group("urepo") or repo
    num = int(match.group("num") or match.group("unum"))
    key = (ref_repo.lower(), num)
    display = f"#{num}" if ref_repo.lower() == repo.lower() else f"{ref_repo}#{num}"
    return key, display


def _line_of(text: str, start: int, end: int) -> str:
    line_start = text.rfind("\n", 0, start) + 1
    line_end = text.find("\n", end)
    line = text[line_start:] if line_end == -1 else text[line_start:line_end]
    line = " ".join(line.split())
    return line if len(line) <= _LINE_LIMIT else line[:_LINE_LIMIT - 3] + "..."


def _is_negated(text: str, start: int, end: int) -> bool:
    """Whether the clause containing a closing reference carries a negation, on either side.

    The clause runs from the nearest boundary before the keyword to the nearest one after the
    reference, across soft line breaks. Both sides count: "Fixes #5, but does not close it" closes
    #5 as surely as "does not close #5" does. Over the last 100 merged PRs no genuine close had a
    negation later in its own clause, so reading forward cost no false block there.

    A capitalised keyword opening its own line gets no exemption: "does not" at the end of one
    line and "Close #5" on the next may be a wrapped sentence, and GitHub closes #5 either way. An
    earlier version exempted that shape as a trailer; over the same 100 PRs the exemption touched
    40 references and prevented no false block, while it let a negated close through silently.
    """
    before = text[:start]
    boundaries = list(_CLAUSE_BOUNDARY.finditer(before))
    clause_before = before[boundaries[-1].end():] if boundaries else before
    after = text[end:]
    boundary = _CLAUSE_BOUNDARY.search(after)
    clause_after = after[:boundary.start()] if boundary else after
    return bool(_NEGATION.search((clause_before + " " + clause_after).translate(_APOSTROPHES)))


def _references(repo: str, text: str):
    """Yield (key, display, negated, line) for every closing reference in text."""
    for match in _CLOSING.finditer(text or ""):
        key, display = _normalise(repo, match)
        yield key, display, _is_negated(text, match.start(), match.end()), _line_of(text, match.start(), match.end())


def closing_problems(repo: str, title: str, body: str, commit_messages) -> list[str]:
    """Landing blockers for closing keywords; an empty list means the text is safe to land.

    `commit_messages` holds either plain message strings or `(label, message)` pairs.
    """
    problems: list[str] = []
    declared: set[tuple[str, int]] = set()

    for key, display, negated, line in _references(repo, body):
        if negated:
            problems.append(
                f"negated closing keyword in PR body still closes {display}; "
                f'GitHub ignores negation: "{line}"'
            )
        else:
            declared.add(key)

    sources = [("title", title or "")]
    for index, item in enumerate(commit_messages, start=1):
        label, message = item if isinstance(item, tuple) else (f"commit {index}", item)
        sources.append((label, message or ""))

    for label, text in sources:
        for key, display, negated, line in _references(repo, text):
            if negated:
                problems.append(
                    f"negated closing keyword in {label} still closes {display}; "
                    f'GitHub ignores negation: "{line}"'
                )
            elif key not in declared:
                problems.append(
                    f"{label} closes {display}, which the PR body does not declare; "
                    f'declare it in the body or drop the keyword: "{line}"'
                )
    return problems
