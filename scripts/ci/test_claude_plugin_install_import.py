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
from pathlib import Path
from types import ModuleType

ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / "scripts/ci/claude_plugin_install_workflow.py"
GUARD = 'if __name__ == "__main__":'
MAIN_LINE = "raise SystemExit(main())"
AUTH_ENV_PREFIXES = ("ANTHROPIC_", "CLAUDE_CODE_OAUTH_", "ASSAY_AUTH_")


class LaunchAttempt(RuntimeError):
    """A trapped external launch. Never forwarded to a real process."""


def _auth_env() -> dict[str, str]:
    return {
        key: value
        for key, value in os.environ.items()
        if key.upper().startswith(AUTH_ENV_PREFIXES)
    }


class ImportTraps:
    def __init__(self) -> None:
        self.launches: list[object] = []
        self._original_popen = subprocess.Popen
        self._original_run = subprocess.run
        self._original_call = subprocess.call
        self._original_check_call = subprocess.check_call
        self._original_check_output = subprocess.check_output
        self._original_system = os.system
        self._original_execv = os.execv
        self._original_execve = os.execve
        self._original_which = shutil.which

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

        def which(cmd: str, *args: object, **kwargs: object) -> str | None:
            # Never hand a real Claude/MCP binary to an accidental import-time verify.
            if cmd in {"claude", "assay-mcp-server"}:
                return None
            return self._original_which(cmd, *args, **kwargs)

        subprocess.Popen = popen  # type: ignore[assignment]
        subprocess.run = run  # type: ignore[assignment]
        subprocess.call = call  # type: ignore[assignment]
        subprocess.check_call = check_call  # type: ignore[assignment]
        subprocess.check_output = check_output  # type: ignore[assignment]
        os.system = system  # type: ignore[assignment]
        os.execv = execv  # type: ignore[assignment]
        os.execve = execve  # type: ignore[assignment]
        shutil.which = which  # type: ignore[assignment]
        return self

    def __exit__(self, *exc: object) -> None:
        subprocess.Popen = self._original_popen
        subprocess.run = self._original_run
        subprocess.call = self._original_call
        subprocess.check_call = self._original_check_call
        subprocess.check_output = self._original_check_output
        os.system = self._original_system
        os.execv = self._original_execv
        os.execve = self._original_execve
        shutil.which = self._original_which


def load_workflow(path: Path, module_name: str) -> ModuleType:
    spec = importlib.util.spec_from_file_location(module_name, path)
    if spec is None or spec.loader is None:
        raise SystemExit(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    return module


class ClaudePluginInstallImportTests(unittest.TestCase):
    def test_import_does_not_execute_workflow(self) -> None:
        auth_before = _auth_env()
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
                    f"import attempted an external launch before any process started: "
                    f"{error}; launches={traps.launches}"
                )
        self.assertEqual(traps.launches, [])
        self.assertEqual(_auth_env(), auth_before)
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
            # External launch must not have escaped the trap.
            for channel, _payload in traps.launches:
                self.assertIn(channel, {"Popen", "run", "call", "check_call", "check_output", "system", "execv", "execve"})


if __name__ == "__main__":
    unittest.main()
