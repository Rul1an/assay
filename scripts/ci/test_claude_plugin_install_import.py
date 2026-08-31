#!/usr/bin/env python3
"""Importing the Claude plugin workflow must not execute it."""

from __future__ import annotations

import importlib.util
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from contextlib import ExitStack
from pathlib import Path
from types import ModuleType
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / "scripts/ci/claude_plugin_install_workflow.py"
GUARD = 'if __name__ == "__main__":'
MAIN_LINE = "raise SystemExit(main())"
AUTH_ENV_PREFIXES = ("ANTHROPIC_", "CLAUDE_CODE_OAUTH_", "ASSAY_AUTH_")
SYNTHETIC_AUTH = {
    "ANTHROPIC_API_KEY": "synthetic-canary-2690-anthropic",
    "CLAUDE_CODE_OAUTH_TOKEN": "synthetic-canary-2690-oauth",
    "ASSAY_AUTH_TOKEN": "synthetic-canary-2690-assay",
}


class LaunchAttempt(RuntimeError):
    """A trapped external launch. Never forwarded to a real process."""


def _synthetic_environ() -> dict[str, str]:
    env = {
        key: value
        for key, value in os.environ.items()
        if not key.upper().startswith(AUTH_ENV_PREFIXES)
    }
    env.update(SYNTHETIC_AUTH)
    return env


def _auth_keys() -> tuple[str, ...]:
    return tuple(sorted(key for key in os.environ if key.upper().startswith(AUTH_ENV_PREFIXES)))


def auth_drift_message(before_keys: tuple[str, ...], after_keys: tuple[str, ...], changed: list[str]) -> str:
    """Name drifted keys only. Never interpolate credential values."""
    added = sorted(set(after_keys) - set(before_keys))
    removed = sorted(set(before_keys) - set(after_keys))
    return (
        "synthetic auth env drifted: "
        f"added={added} removed={removed} changed={changed}"
    )


class ImportTraps:
    def __init__(self) -> None:
        self.launches: list[tuple[str, object]] = []
        self._stack = ExitStack()

    def _trap(self, channel: str, payload: object) -> None:
        self.launches.append((channel, payload))
        raise LaunchAttempt(f"{channel}:{payload!r}")

    def __enter__(self) -> ImportTraps:
        def popen(*args: object, **kwargs: object) -> None:
            self._trap("Popen", args[0] if args else kwargs)

        def run(*args: object, **kwargs: object) -> None:
            self._trap("run", args[0] if args else kwargs)

        def call(*args: object, **kwargs: object) -> None:
            self._trap("call", args[0] if args else kwargs)

        def check_call(*args: object, **kwargs: object) -> None:
            self._trap("check_call", args[0] if args else kwargs)

        def check_output(*args: object, **kwargs: object) -> None:
            self._trap("check_output", args[0] if args else kwargs)

        def system(*args: object, **kwargs: object) -> None:
            self._trap("system", args[0] if args else kwargs)

        def execv(*args: object, **kwargs: object) -> None:
            self._trap("execv", args[0] if args else kwargs)

        def execve(*args: object, **kwargs: object) -> None:
            self._trap("execve", args[0] if args else kwargs)

        real_which = shutil.which

        def which(cmd: str, *args: object, **kwargs: object) -> str | None:
            if cmd in {"claude", "assay-mcp-server"}:
                return None
            return real_which(cmd, *args, **kwargs)

        self._stack.enter_context(patch.object(subprocess, "Popen", popen))
        self._stack.enter_context(patch.object(subprocess, "run", run))
        self._stack.enter_context(patch.object(subprocess, "call", call))
        self._stack.enter_context(patch.object(subprocess, "check_call", check_call))
        self._stack.enter_context(patch.object(subprocess, "check_output", check_output))
        self._stack.enter_context(patch.object(os, "system", system))
        self._stack.enter_context(patch.object(os, "execv", execv))
        self._stack.enter_context(patch.object(os, "execve", execve))
        self._stack.enter_context(patch.object(shutil, "which", which))
        return self

    def __exit__(self, *exc: object) -> bool | None:
        return self._stack.__exit__(*exc)


def load_workflow(path: Path, module_name: str) -> ModuleType:
    spec = importlib.util.spec_from_file_location(module_name, path)
    if spec is None or spec.loader is None:
        raise SystemExit(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    return module


class ClaudePluginInstallImportTests(unittest.TestCase):
    def setUp(self) -> None:
        self.enterContext(patch.dict(os.environ, _synthetic_environ(), clear=True))

    def _assert_synthetic_auth_unchanged(self) -> None:
        after_keys = _auth_keys()
        expected_keys = tuple(sorted(SYNTHETIC_AUTH))
        changed = [
            key
            for key in SYNTHETIC_AUTH
            if os.environ.get(key) != SYNTHETIC_AUTH[key]
        ]
        if after_keys != expected_keys or changed:
            self.fail(auth_drift_message(expected_keys, after_keys, changed))

    def test_import_does_not_execute_workflow(self) -> None:
        traps = ImportTraps()
        with traps:
            try:
                module = load_workflow(
                    WORKFLOW, "claude_plugin_install_workflow_import_safe"
                )
            except SystemExit as error:
                self.fail(
                    f"import executed main() (SystemExit={error.code}); "
                    f"launches={traps.launches}"
                )
            except LaunchAttempt as error:
                self.fail(
                    "import attempted an external launch before any process started: "
                    f"{error}; launches={traps.launches}"
                )
        self.assertEqual(traps.launches, [])
        self._assert_synthetic_auth_unchanged()
        self.assertTrue(callable(module.main))
        self.assertTrue(callable(module.verify_workflow))
        self.assertTrue(callable(module.self_test))

    def test_removing_the_main_guard_makes_import_execute_main(self) -> None:
        source = WORKFLOW.read_text(encoding="utf-8")
        self.assertIn(GUARD, source)
        unguarded = source.replace(
            f"{GUARD}\n    {MAIN_LINE}\n",
            f"{MAIN_LINE}\n",
            1,
        )
        self.assertNotEqual(unguarded, source)
        self.assertNotIn(GUARD, unguarded)
        with tempfile.TemporaryDirectory(prefix="assay-claude-import-mut-") as temporary:
            mutant = Path(temporary) / "claude_plugin_install_workflow.py"
            mutant.write_text(unguarded, encoding="utf-8")
            traps = ImportTraps()
            with traps:
                with self.assertRaises((SystemExit, LaunchAttempt)) as raised:
                    load_workflow(mutant, "claude_plugin_install_workflow_unguarded")
            self.assertIsInstance(raised.exception, (SystemExit, LaunchAttempt))
            for channel, _payload in traps.launches:
                self.assertIn(
                    channel,
                    {
                        "Popen",
                        "run",
                        "call",
                        "check_call",
                        "check_output",
                        "system",
                        "execv",
                        "execve",
                    },
                )
            self._assert_synthetic_auth_unchanged()

    def test_auth_drift_diagnostic_omits_values(self) -> None:
        message = auth_drift_message(
            ("ANTHROPIC_API_KEY",),
            ("ANTHROPIC_API_KEY", "ASSAY_AUTH_TOKEN"),
            ["ANTHROPIC_API_KEY"],
        )
        self.assertEqual(
            message,
            "synthetic auth env drifted: added=['ASSAY_AUTH_TOKEN'] "
            "removed=[] changed=['ANTHROPIC_API_KEY']",
        )
        for value in SYNTHETIC_AUTH.values():
            self.assertNotIn(value, message)
        os.environ["ANTHROPIC_API_KEY"] = "synthetic-canary-2690-mutated"
        with self.assertRaises(AssertionError) as raised:
            self._assert_synthetic_auth_unchanged()
        diagnostic = str(raised.exception)
        self.assertIn("changed=['ANTHROPIC_API_KEY']", diagnostic)
        self.assertNotIn("synthetic-canary-2690-mutated", diagnostic)
        self.assertNotIn(SYNTHETIC_AUTH["ANTHROPIC_API_KEY"], diagnostic)


if __name__ == "__main__":
    unittest.main()
