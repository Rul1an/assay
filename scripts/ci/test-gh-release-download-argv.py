#!/usr/bin/env python3
"""Refuse positional assets on `gh release download` in workflow `run:` scripts.

`gh release download [<tag>] [flags]` accepts at most one positional. Assets are
selected with `-p/--pattern`, never as extra argv words. Release run 34727302233
job 103645127255 died with `accepts at most 1 arg(s), received 7` because
`.github/workflows/release.yml` passed six asset names after the tag.

This is a text-shape guard over `.github/workflows/*.ya?ml`. It is the right
tool: the failure is argv shape, rejected before any network call. Executing
real `gh` would test the CLI, not the workflow text that produced the 7-arg
line.

Documented self-test (entrypoint is at the bottom of this file):

    python3 scripts/ci/test-gh-release-download-argv.py

`unittest` verbosity 2 prints every case. `DECLARED_TEST_COUNT` must match
the collected suite size; an entrypoint sitting mid-file would run a prefix
and still print OK.

Scope is workflows only. A doc line is not an execution path, `scripts/` has
zero hits, and the conformance/docs sites already use `--pattern`. The
`files:` regex on the pre-commit hook matches this script plus
`.github/workflows/.*\\.ya?ml`.

Seams:

* Line continuations ending in `\\` are joined before tokens are counted. A
  line-oriented matcher sees only `gh release download "$VERSION" --repo ... \\`
  and reports clean.
* Value-taking flags are taken from `gh release download --help` (quoted
  below). A naive "count tokens that do not start with `-`" treats
  `--dir "$asset_dir"` as a second positional and fires on every correct site.
* `run:` blocks are YAML scalars. This scans raw file text, not a YAML parse:
  the executed argv is the source text Actions echoes, original line numbers
  are native, and a YAML load would still need the same `\\` joiner for `|`
  blocks. Folded `>` scalars with a positional on the next source line are a
  non-claim; no current site uses that form. Only command-position `gh`
  counts, so a step name such as `Verify gh release download argv contract`
  is not an invocation. A line whose first non-space character is `#` is a
  comment, not an invocation: `&&` after `#` must stay quiet. Trailing `#`
  comments are stripped quote-aware before token counting. Invocations prefixed
  by `sudo`, `env`, variable assignments, single-line `run:`, pipes, or
  subshells/backticks are recognized.

Value-taking flags, from `gh release download --help`:

    -R, --repo [HOST/]OWNER/REPO
    -D, --dir directory
    -p, --pattern stringArray
    -O, --output file
    -A, --archive format

`--clobber` and `--skip-existing` do not take a value. `--help` does not.
`--flag=value` does not consume the next token.
"""
from __future__ import annotations

import re
import shlex
import sys
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
WORKFLOW_DIR = REPO / ".github/workflows"

# From `gh release download --help`. Keep in lockstep with that listing.
VALUE_FLAGS = frozenset(
    {
        "-R",
        "--repo",
        "-D",
        "--dir",
        "-p",
        "--pattern",
        "-O",
        "--output",
        "-A",
        "--archive",
    }
)

# Command position: start of line (optionally YAML list item or `run:`), shell
# list/pipe operators, or subshells/command substitutions, followed by optional
# command prefixes (`sudo`, `env`, `VAR=value` assignments, `!`).
# A YAML step name or hook id that mentions the phrase is not an invocation.
INVOKE_RE = re.compile(
    r"""
    (?:
        ^\s*(?:-\s+)?(?:run:\s*)?   # start of line, optional YAML list item, optional run:
      | (?:&&|\|\||;|\|&|\||&)\s*   # shell command separators
      | (?:\$\(|\`|\()\s*           # command substitution or subshell
    )
    (?:
        (?:
            sudo\s+(?:(?!--|\bgh\b)\S+\s+)*(?:--\s+)?
          | env\s+(?:(?!--|\bgh\b)\S+\s+)*(?:--\s+)?
          | [A-Za-z_]\w*=(?:"[^"]*"|'[^']*'|\S*)\s+
          | !\s+
        )
    )*
    gh\s+release\s+download\b
    """,
    re.VERBOSE | re.MULTILINE,
)
COMMAND_TERMINATORS = frozenset(
    {
        "|",
        "|&",
        "&&",
        "||",
        ";",
        ";;",
        "&",
        ")",
        "`",
    }
)
DECLARED_TEST_COUNT = 21

PUBLISH_HEAD = (
    '          gh release download "$VERSION" --repo "$GITHUB_REPOSITORY"'
    ' --dir "$asset_dir" \\\n'
)
PUBLISH_ASSETS = (
    "assay-mcp-server-${VERSION}-x86_64-unknown-linux-gnu.tar.gz",
    "assay-mcp-server-${VERSION}-x86_64-unknown-linux-gnu.tar.gz.sha256",
    "assay-mcp-server-${VERSION}-aarch64-unknown-linux-gnu.tar.gz",
    "assay-mcp-server-${VERSION}-aarch64-unknown-linux-gnu.tar.gz.sha256",
    "assay-${VERSION}-sbom-cyclonedx.tar.gz",
    "assay-${VERSION}-sbom-cyclonedx.tar.gz.sha256",
)
VERIFY_POSITIONAL_TAIL = '            "$archive" "${archive}.sha256"\n'
VERIFY_PATTERN_TAIL = (
    '            --pattern "$archive" \\\n'
    '            --pattern "${archive}.sha256"\n'
)
MCP_PATTERN = "            --pattern server.json \\\n"
MCP_POSITIONAL = "            server.json \\\n"


def joined_lines(text: str) -> list[tuple[int, str]]:
    """Logical lines after joining a trailing `\\` continuation."""
    raw = text.splitlines()
    out: list[tuple[int, str]] = []
    i = 0
    while i < len(raw):
        start = i + 1
        chunk = raw[i]
        while chunk.rstrip().endswith("\\"):
            chunk = chunk.rstrip()[:-1]
            i += 1
            if i >= len(raw):
                break
            chunk = chunk + " " + raw[i].lstrip()
        out.append((start, chunk))
        i += 1
    return out


def positional_tokens(tokens: list[str]) -> list[str]:
    positionals: list[str] = []
    i = 0
    while i < len(tokens):
        token = tokens[i]
        if token in COMMAND_TERMINATORS or token.startswith(("<", ">")):
            break
        if (
            token.isdigit()
            and i + 1 < len(tokens)
            and tokens[i + 1].startswith(("<", ">"))
        ):
            break
        if token == "--":
            for t in tokens[i + 1 :]:
                if t in COMMAND_TERMINATORS or t.startswith(("<", ">")):
                    break
                positionals.append(t)
            break
        if token.startswith("-"):
            name = token.split("=", 1)[0]
            if "=" in token or name not in VALUE_FLAGS:
                i += 1
                continue
            if i + 1 >= len(tokens):
                raise ValueError(f"value-taking flag {token} is missing its argument")
            i += 2
            continue
        positionals.append(token)
        i += 1
    return positionals


def strip_shell_comment(text: str) -> str:
    """Strip trailing shell comment (# preceded by whitespace or at start of word)."""
    in_single = False
    in_double = False
    escape = False
    at_word_start = True
    for i, ch in enumerate(text):
        if escape:
            escape = False
            continue
        if ch == "\\" and not in_single:
            escape = True
            at_word_start = False
            continue
        if in_single:
            if ch == "'":
                in_single = False
            continue
        if in_double:
            if ch == '"':
                in_double = False
            continue
        if ch == "'":
            in_single = True
            at_word_start = False
        elif ch == '"':
            in_double = True
            at_word_start = False
        elif ch.isspace():
            at_word_start = True
        elif ch == "#" and at_word_start:
            return text[:i]
        else:
            at_word_start = False
    return text


def tokenize_invocation(rest: str) -> list[str]:
    rest = strip_shell_comment(rest)
    lex = shlex.shlex(rest, posix=True, punctuation_chars=True)
    lex.whitespace_split = True
    lex.commenters = ""
    return list(lex)


def invocation_problems(text: str, relpath: str) -> list[str]:
    problems: list[str] = []
    for line_no, line in joined_lines(text):
        if line.lstrip().startswith("#"):
            continue
        for match in INVOKE_RE.finditer(line):
            rest = line[match.end() :]
            try:
                tokens = tokenize_invocation(rest)
                positionals = positional_tokens(tokens)
            except ValueError as exc:
                problems.append(f"{relpath}:{line_no}: cannot tokenize: {exc}")
                continue
            if len(positionals) > 1:
                problems.append(
                    f"{relpath}:{line_no}: gh release download accepts at most 1 "
                    f"positional (the tag); received {len(positionals)}: "
                    f"{positionals!r}. Select assets with --pattern."
                )
    return problems


def workflow_paths(root: Path = REPO) -> list[Path]:
    directory = root / ".github/workflows"
    if not directory.is_dir():
        raise FileNotFoundError(directory)
    return sorted(directory.glob("*.yml")) + sorted(directory.glob("*.yaml"))


def live_problems(root: Path = REPO) -> list[str]:
    problems: list[str] = []
    for path in workflow_paths(root):
        rel = path.relative_to(root).as_posix()
        problems.extend(invocation_problems(path.read_text(encoding="utf-8"), rel))
    return problems


def continued(head: str, *assets: str, pattern: bool) -> str:
    lines = [head]
    for index, asset in enumerate(assets):
        prefix = "--pattern " if pattern else ""
        suffix = " \\" if index + 1 < len(assets) else ""
        lines.append(f'            {prefix}"{asset}"{suffix}\n')
    return "".join(lines)


PUBLISH_POSITIONAL = continued(PUBLISH_HEAD, *PUBLISH_ASSETS, pattern=False)
PUBLISH_PATTERN = continued(PUBLISH_HEAD, *PUBLISH_ASSETS, pattern=True)


def replace_one(text: str, old: str, new: str) -> str:
    count = text.count(old)
    if count != 1:
        raise AssertionError(f"anchor count for {old!r}: {count}")
    return text.replace(old, new, 1)


def force_block(text: str, positional: str, patterned: str) -> str:
    if positional in text:
        return text
    if patterned in text:
        return text.replace(patterned, positional, 1)
    raise AssertionError("neither positional nor --pattern form of the block is present")


class GhReleaseDownloadArgv(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.release = (REPO / ".github/workflows/release.yml").read_text(encoding="utf-8")
        cls.registry = (
            REPO / ".github/workflows/mcp-registry-publish.yml"
        ).read_text(encoding="utf-8")

    def test_declared_suite_size(self) -> None:
        loader = unittest.TestLoader()
        suite = loader.loadTestsFromModule(sys.modules[__name__])
        self.assertEqual(suite.countTestCases(), DECLARED_TEST_COUNT)

    def test_live_workflows_have_at_most_one_positional(self) -> None:
        problems = live_problems()
        self.assertEqual(problems, [], "\n".join(problems))

    def test_publish_image_positional_assets_are_refused(self) -> None:
        mutant = force_block(self.release, PUBLISH_POSITIONAL, PUBLISH_PATTERN)
        problems = invocation_problems(mutant, ".github/workflows/release.yml")
        self.assertTrue(problems, "positional publish-image download survived")
        self.assertTrue(
            any(
                item.startswith(".github/workflows/release.yml:752:")
                for item in problems
            ),
            problems,
        )

    def test_verify_image_positional_assets_are_refused(self) -> None:
        mutant = force_block(
            self.release,
            PUBLISH_HEAD + VERIFY_POSITIONAL_TAIL,
            PUBLISH_HEAD + VERIFY_PATTERN_TAIL,
        )
        problems = invocation_problems(mutant, ".github/workflows/release.yml")
        self.assertTrue(problems, "positional verify-image download survived")
        self.assertTrue(
            any(
                item.startswith(".github/workflows/release.yml:903:")
                for item in problems
            ),
            problems,
        )

    def test_mcp_registry_positional_asset_is_refused(self) -> None:
        mutant = replace_one(self.registry, MCP_PATTERN, MCP_POSITIONAL)
        problems = invocation_problems(mutant, ".github/workflows/mcp-registry-publish.yml")
        self.assertTrue(problems, "positional mcp-registry download survived")
        self.assertTrue(
            any(
                item.startswith(".github/workflows/mcp-registry-publish.yml:91:")
                for item in problems
            ),
            problems,
        )

    def test_positional_asset_on_continuation_line_is_refused(self) -> None:
        text = (
            '          gh release download "$TAG" --repo "$GITHUB_REPOSITORY" \\\n'
            "            leftover-asset.tar.gz\n"
        )
        problems = invocation_problems(text, "seam-continuation.yml")
        self.assertEqual(len(problems), 1, problems)
        self.assertTrue(problems[0].startswith("seam-continuation.yml:1:"), problems)

    def test_dir_flag_value_is_not_counted_as_positional(self) -> None:
        text = (
            '          gh release download "$TAG" --repo "$GITHUB_REPOSITORY" '
            '--dir "$asset_dir" --pattern server.json\n'
        )
        self.assertEqual(invocation_problems(text, "seam-dir.yml"), [])
        live = invocation_problems(self.registry, ".github/workflows/mcp-registry-publish.yml")
        self.assertEqual(live, [])

    def test_step_name_mention_is_not_an_invocation(self) -> None:
        text = "      - name: Verify gh release download argv contract\n"
        self.assertEqual(invocation_problems(text, ".github/workflows/ci.yml"), [])

    def test_comment_line_is_not_an_invocation(self) -> None:
        text = "          # && gh release download x y\n"
        self.assertEqual(invocation_problems(text, "seam-comment.yml"), [])

    def test_trailing_comment_compliant_is_quiet(self) -> None:
        text = (
            '          gh release download "$TAG" --pattern a.tar.gz  # refresh it\n'
            '          gh release download "$TAG" --pattern "a#b.tar.gz"  # with quoted hash\n'
        )
        self.assertEqual(invocation_problems(text, "seam-trailing-comment.yml"), [])

    def test_trailing_comment_positional_asset_is_refused(self) -> None:
        text = '          gh release download "$TAG" a.tar.gz # note\n'
        problems = invocation_problems(text, "seam-trailing-comment-violation.yml")
        self.assertEqual(len(problems), 1, problems)
        self.assertTrue(problems[0].startswith("seam-trailing-comment-violation.yml:1:"), problems)

    def test_sudo_prefix_positional_asset_is_refused(self) -> None:
        bad = '          sudo -u runner -E gh release download "$TAG" a.tar.gz\n'
        good = '          sudo -u runner -E gh release download "$TAG" --pattern a.tar.gz\n'
        self.assertEqual(len(invocation_problems(bad, "seam-sudo.yml")), 1)
        self.assertEqual(invocation_problems(good, "seam-sudo.yml"), [])

    def test_var_assignment_prefix_positional_asset_is_refused(self) -> None:
        bad = '          GH_TOKEN=xyz VAR="foo bar" gh release download "$TAG" a.tar.gz\n'
        good = '          GH_TOKEN=xyz VAR="foo bar" gh release download "$TAG" --pattern a.tar.gz\n'
        self.assertEqual(len(invocation_problems(bad, "seam-var.yml")), 1)
        self.assertEqual(invocation_problems(good, "seam-var.yml"), [])

    def test_env_command_prefix_positional_asset_is_refused(self) -> None:
        bad = '          env -i GH_TOKEN=xyz gh release download "$TAG" a.tar.gz\n'
        good = '          env -i GH_TOKEN=xyz gh release download "$TAG" --pattern a.tar.gz\n'
        self.assertEqual(len(invocation_problems(bad, "seam-env.yml")), 1)
        self.assertEqual(invocation_problems(good, "seam-env.yml"), [])

    def test_pipe_prefix_positional_asset_is_refused(self) -> None:
        bad = '          echo "$TAG" | gh release download "$TAG" a.tar.gz\n'
        good = '          echo "$TAG" | gh release download "$TAG" --pattern a.tar.gz\n'
        self.assertEqual(len(invocation_problems(bad, "seam-pipe.yml")), 1)
        self.assertEqual(invocation_problems(good, "seam-pipe.yml"), [])

    def test_command_substitution_positional_asset_is_refused(self) -> None:
        bad_sub = '          TAG=$(gh release download "$TAG" a.tar.gz)\n'
        good_sub = '          TAG=$(gh release download "$TAG" --pattern a.tar.gz)\n'
        bad_bt = '          TAG=`gh release download "$TAG" a.tar.gz`\n'
        good_bt = '          TAG=`gh release download "$TAG" --pattern a.tar.gz`\n'
        self.assertEqual(len(invocation_problems(bad_sub, "seam-subshell.yml")), 1)
        self.assertEqual(invocation_problems(good_sub, "seam-subshell.yml"), [])
        self.assertEqual(len(invocation_problems(bad_bt, "seam-backtick.yml")), 1)
        self.assertEqual(invocation_problems(good_bt, "seam-backtick.yml"), [])

    def test_single_line_run_positional_asset_is_refused(self) -> None:
        bad = '      - run: gh release download "$TAG" a.tar.gz\n'
        good = '      - run: gh release download "$TAG" --pattern a.tar.gz\n'
        self.assertEqual(len(invocation_problems(bad, "seam-run.yml")), 1)
        self.assertEqual(invocation_problems(good, "seam-run.yml"), [])

    def test_hash_in_dir_value_positional_asset_is_refused(self) -> None:
        text = '          gh release download "$TAG" --dir out#1 a.tar.gz\n'
        problems = invocation_problems(text, "seam-dir-hash.yml")
        self.assertEqual(len(problems), 1, problems)
        self.assertTrue(problems[0].startswith("seam-dir-hash.yml:1:"), problems)

    def test_hash_glued_to_tag_positional_asset_is_refused(self) -> None:
        text = '          gh release download "$TAG"#x a.tar.gz\n'
        problems = invocation_problems(text, "seam-tag-hash.yml")
        self.assertEqual(len(problems), 1, problems)
        self.assertTrue(problems[0].startswith("seam-tag-hash.yml:1:"), problems)

    def test_hash_in_pattern_flag_value_is_quiet_and_single_token(self) -> None:
        text = '          gh release download "$TAG" --pattern a.tar.gz#x\n'
        self.assertEqual(invocation_problems(text, "seam-pattern-hash.yml"), [])
        tokens = tokenize_invocation(' "$TAG" --pattern a.tar.gz#x')
        self.assertEqual(tokens, ["$TAG", "--pattern", "a.tar.gz#x"])

    def test_hook_id_mention_is_not_an_invocation(self) -> None:
        text = "      - id: gh-release-download-argv\n"
        self.assertEqual(invocation_problems(text, ".pre-commit-config.yaml"), [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
