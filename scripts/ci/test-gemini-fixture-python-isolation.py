#!/usr/bin/env python3
"""Exercise the real wrapper against a polluted Python installation, offline."""

import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import venv

ROOT = Path(__file__).resolve().parents[2]
WRAPPER = ROOT / "runner-fixtures/gemini-google-genai/sdk-policy-agent.sh"


class PythonIsolation(unittest.TestCase):
    def test_local_dependencies_exclude_site_packages_but_venv_fallback_works(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            venv.EnvBuilder(with_pip=False).create(root / "venv")
            python = root / "venv/bin/python"
            site = Path(subprocess.check_output(
                [str(python), "-c", "import sysconfig; print(sysconfig.get_path('purelib'))"],
                text=True,
            ).strip())
            (site / "unrelated_host_package.py").write_text("VALUE = 'host'\n")
            deps = root / "deps"
            deps.mkdir()
            (deps / "fixture_dependency.py").write_text("VALUE = 'local'\n")
            probe = root / "probe.py"
            probe.write_text(
                "import importlib.util, os\n"
                "isolated = os.environ['EXPECT_ISOLATED'] == '1'\n"
                "present = importlib.util.find_spec('unrelated_host_package') is not None\n"
                "assert present != isolated, 'ambient site-packages leaked into fixture'\n"
                "if isolated:\n"
                "    from fixture_dependency import VALUE\n"
                "    assert VALUE == 'local'\n"
            )
            extract = root / "extract.py"
            extract.write_text(probe.read_text() + "print('recorded-id')\n")
            policy = root / "policy.sh"
            policy.write_text('#!/bin/sh\ntest "$ASSAY_RUNNER_SDK_TOOL_CALL_ID" = recorded-id\n')
            policy.chmod(0o755)
            env = {k: v for k, v in os.environ.items() if k not in {"PYTHONPATH", "PYTHONHOME"}}
            env.update(ASSAY_RUNNER_GEMINI_FIXTURE_PYTHON=str(python),
                       ASSAY_RUNNER_GEMINI_FIXTURE_SCRIPT=str(probe),
                       ASSAY_RUNNER_GEMINI_EXTRACT_SCRIPT=str(extract),
                       ASSAY_RUNNER_GEMINI_POLICY_WRAPPER=str(policy),
                       ASSAY_RUNNER_PHASE_DRAIN_SLEEP="0")
            for isolated in (True, False):
                with self.subTest(local_dependencies=isolated):
                    env.update(EXPECT_ISOLATED=str(int(isolated)),
                               ASSAY_RUNNER_GEMINI_PYTHONPATH=str(deps if isolated else root / "absent"))
                    result = subprocess.run(["bash", str(WRAPPER), str(root)], env=env,
                                            capture_output=True, text=True, timeout=20)
                    self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
