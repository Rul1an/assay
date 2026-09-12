#!/usr/bin/env python3
"""The findings diagnostic channel on one execution chain.

    python3 conformance/tests/test_privileged_mcp_diagnostic_channel.py

Box 4 of issue #2862: a finding-only mutation must survive with `findings`
undeclared and become silent once `findings` is declared, on the SAME chain
(the pinned instrument's process runner, one manifest shape, one control).
The positive control flips a normative outcome and must read killed in both
runs; it is asserted FIRST, so a barrier refusal or a missing row cannot
masquerade as the intended survived/silent verdict.

This runs the pinned instrument, not a reimplementation of its meaning: the
tool checkout must sit beside this repository at exactly the commit the
privileged-mcp-action-v0 manifest pins, else the test skips loudly rather
than measuring with the wrong instrument.
"""

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
MANIFEST = REPO / "conformance/adequacy/privileged-mcp-action-v0.manifest.json"

_TOOL = REPO.parent / "corpus-adequacy"
if _TOOL.is_dir():
    sys.path.insert(0, str(_TOOL))
try:
    import corpus_adequacy as ca  # noqa: E402
except ImportError:  # pragma: no cover - environment without the sibling checkout
    ca = None


def _tool_head() -> str | None:
    import subprocess
    out = subprocess.run(["git", "-C", str(_TOOL), "rev-parse", "HEAD"],
                         capture_output=True, text=True)
    return out.stdout.strip() if out.returncode == 0 else None


def _pinned_tool() -> str:
    return json.loads(MANIFEST.read_text(encoding="utf-8"))["tool_pin"]["commit"]


def _needs_pinned_tool() -> str | None:
    if ca is None:
        return ("corpus-adequacy not found as a sibling checkout; clone "
                "https://github.com/corpus-adequacy/corpus-adequacy next to this repository")
    if _tool_head() != _pinned_tool():
        return ("corpus-adequacy is at %s, not the manifest-pinned %s; "
                "this test measures only with the pinned instrument"
                % (_tool_head(), _pinned_tool()))
    if ca.fcntl is None:
        return "process scoring requires an advisory lock"
    return None


_SKIP = _needs_pinned_tool()

_IMPLEMENTATION = (
    "import json, sys\n"
    "vector = json.load(open(sys.argv[1]))\n"
    "assert vector[\"intact\"] is True\n"
    "ok = True\n"
    'note = \"A\"\n'
    "print(json.dumps({\n"
    '    \"verdict\": \"valid\" if ok else \"invalid\",\n'
    '    \"findings\": [{\"id\": \"note\", \"detail\": note}],\n'
    "}))\n"
)


def _mini_manifest(tmp: Path, extra: dict | None = None) -> Path:
    (tmp / "check.py").write_text(_IMPLEMENTATION, encoding="utf-8")
    (tmp / "vec.json").write_text('{"intact": true}\n', encoding="utf-8")
    (tmp / "vectors.json").write_text(json.dumps({
        "vectors": [{"vector_id": "v1", "path": "vec.json"}]}), encoding="utf-8")
    raw = {
        "schema": ca.SCHEMA, "runner": "process", "repo_root": ".",
        "implementation": "check.py", "implementation_sources": ["check.py"],
        "build": [],
        "entrypoint_command": [sys.executable, "check.py", "{vector}"],
        "outcome_from": ["verdict"], "vectors": "vectors.json",
        "id_key": "vector_id", "vector_path_key": "path",
        "default_group": "g",
        "mutants": {"g": [
            {"label": "finding-only note text",
             "anchor": 'note = "A"',
             "replacement": 'note = "B"'},
            {"label": "CONTROL flips the normative verdict",
             "control": True,
             "anchor": "ok = True",
             "replacement": "ok = False"}]},
    }
    raw.update(dict(extra or {}))
    path = tmp / "mini.manifest.json"
    path.write_text(json.dumps(raw), encoding="utf-8")
    return path


def _run(extra: dict | None = None) -> dict:
    with tempfile.TemporaryDirectory() as raw:
        return ca.run(_mini_manifest(Path(raw), extra))


@unittest.skipIf(_SKIP is not None, _SKIP or "tool unavailable")
class FindingsDiagnosticChannel(unittest.TestCase):
    def _by_label(self, report: dict) -> dict:
        return {row["label"]: row for row in report["mutants"]}

    def test_finding_only_mutation_survives_with_findings_undeclared(self):
        report = _run()
        rows = self._by_label(report)
        # The control first: a quiet harness proves nothing about the mutant.
        control = rows["CONTROL flips the normative verdict"]
        self.assertEqual(control["verdict"], "control-killed")
        self.assertEqual(control["moved"], 1)
        self.assertEqual(report["control_status"], "killed")
        self.assertEqual(rows["finding-only note text"]["verdict"], "survived")
        self.assertEqual(report["silent"], 0)
        self.assertFalse(report["diagnostic_channel_declared"])
        self.assertEqual(report["unproved"], 0)

    def test_finding_only_mutation_is_silent_with_findings_declared(self):
        report = _run({"diagnostic_from": ["findings"]})
        rows = self._by_label(report)
        # The control first, on this chain too: it must still move the outcome.
        control = rows["CONTROL flips the normative verdict"]
        self.assertEqual(
            control["verdict"], "control-killed",
            "declaring the diagnostic channel must not quiet the control")
        self.assertEqual(control["moved"], 1)
        self.assertEqual(report["control_status"], "killed")
        mutant = rows["finding-only note text"]
        self.assertEqual(mutant["verdict"], "silent")
        self.assertEqual(mutant["moved"], 0)
        self.assertEqual(mutant["moved_diagnostic"], 1)
        # The control proves the chain observes outcomes without scoring
        # as an ordinary kill: killed counts only non-control mutants.
        self.assertEqual(report["killed"], 0)
        self.assertEqual(report["silent"], 1)
        self.assertTrue(report["diagnostic_channel_declared"])
        self.assertEqual(report["unproved"], 0)


if __name__ == "__main__":
    unittest.main(verbosity=2)
