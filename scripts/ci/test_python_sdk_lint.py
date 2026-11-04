#!/usr/bin/env python3
"""Exercise the SDK's exception policy through Ruff's committed configuration."""
import json
from pathlib import Path
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
SDK = ROOT / "assay-python-sdk/python/assay/_lint_probe.py"


class SdkExceptionPolicy(unittest.TestCase):
    def lint(self, source):
        result = subprocess.run(
            [sys.executable, "-m", "ruff", "check", "--no-cache",
             "--config", str(ROOT / "assay-python-sdk/pyproject.toml"),
             "--output-format", "json", "--stdin-filename", str(SDK), "-"],
            input=source, text=True, capture_output=True, cwd=ROOT, check=False,
        )
        self.assertIn(result.returncode, (0, 1), result.stderr)
        findings = json.loads(result.stdout)
        self.assertEqual(result.returncode, int(bool(findings)), result.stderr)
        return {finding["code"] for finding in findings}

    def test_sdk_rejects_silent_or_unattributed_exceptions(self):
        cases = [
            ("BLE001", 'try:\n    int("x")\nexcept Exception:\n    print("failed")\n'),
            ("S110", 'try:\n    int("x")\nexcept ValueError:\n    pass\n'),
            ("S112", 'for item in ["x"]:\n    try:\n        int(item)\n    except ValueError:\n        continue\n'),
            ("B904", 'try:\n    int("x")\nexcept ValueError:\n    raise RuntimeError("failed")\n'),
        ]
        for code, source in cases:
            with self.subTest(code=code):
                self.assertEqual(self.lint(source), {code})

    def test_explicit_recovery_and_exception_causes_remain_valid(self):
        sources = [
            'try:\n    int("x")\nexcept ValueError:\n    print("invalid integer")\n',
            'try:\n    int("x")\nexcept Exception:\n    raise\n',
            'try:\n    int("x")\nexcept ValueError as exc:\n    raise RuntimeError("failed") from exc\n',
            'try:\n    int("x")\nexcept ValueError:\n    raise RuntimeError("failed") from None\n',
        ]
        for source in sources:
            with self.subTest(source=source):
                self.assertEqual(self.lint(source), set())

    def test_all_sdk_python_files_are_checked(self):
        result = subprocess.run(
            [sys.executable, "-m", "ruff", "check", "--no-cache",
             "--config", str(ROOT / "assay-python-sdk/pyproject.toml"),
             "--show-files", str(ROOT / "assay-python-sdk/python")],
            text=True, capture_output=True, cwd=ROOT, check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        expected = {str(p) for p in (ROOT / "assay-python-sdk/python").rglob("*")
                    if p.suffix in (".py", ".pyi")}
        self.assertTrue(expected)
        self.assertEqual(set(result.stdout.splitlines()), expected)

if __name__ == "__main__":
    unittest.main()
