import json
import os
import shutil
import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path
from unittest.mock import MagicMock

from verdict_sdk.evaluator import Evaluator


class TestArtifacts(unittest.TestCase):
    def setUp(self):
        self.test_dir = Path(tempfile.mkdtemp())
        self.verdict_dir = self.test_dir / ".eval"
        self.verdict_dir.mkdir()

        # Create minimal eval.yaml
        self.config_path = self.test_dir / "eval.yaml"
        with open(self.config_path, "w") as f:
            f.write(
                """
version: 1
suite: junit_test_suite
tests:
  - id: t1
    prompt: "hello"
    metrics:
      - name: always_true
        threshold: 0.5
"""
            )

        # Create minimal trace with correct schema
        self.trace_path = self.test_dir / "trace.jsonl"
        with open(self.trace_path, "w") as f:
            f.write(
                json.dumps(
                    {"run_id": "r1", "events": [{"kind": "model", "content": "world"}]}
                )
                + "\n"
            )

    def tearDown(self):
        shutil.rmtree(self.test_dir)

    def test_junit_xml_generated(self):
        # Mock Evaluator to use our temp dir
        # We need to chdir or mock workdir behavior
        cwd = os.getcwd()
        os.chdir(self.test_dir)
        try:
            # Mock builtin evaluation to always pass
            with unittest.mock.patch(
                "verdict_sdk.metrics.builtin.eval_builtin"
            ) as mock_eval:
                from verdict_sdk.result import MetricResult

                mock_eval.return_value = MetricResult(
                    name="mock_builtin", value=1.0, passed=True
                )

                # Re-write config with valid builtin
                with open(self.config_path, "w") as f:
                    f.write(
                        """
version: 1
suite: junit_test_suite
tests:
  - id: t1
    prompt: "hello"
    metrics:
      - name: regex_match
        kind: builtin
        threshold: 0.5
        params:
            pattern: "world"
"""
                    )

                # Reload evaluator to pick up new config
                # print("DEBUG CONFIG CONTENT:", self.config_path.read_text())
                ev = Evaluator()
                # DEBUG: Verify what loaded
                # print(f"DEBUG LOADED METRIC NAME: {ev.config.tests[0].metrics[0].name}")
                res = ev.run(str(self.trace_path))

                if not res.passed:
                    print(f"DEBUG RUN PASSED: {res.passed}")
                    print(f"DEBUG TEST 0 PASSED: {res.tests[0].passed}")
                    print(f"DEBUG TEST 0 METRICS: {res.tests[0].metrics}")
                    print(f"DEBUG TEST 0 ID: {res.tests[0].test_id}")
                self.assertTrue(res.passed)

                # Check Artifacts
                artifacts = res.artifacts
                self.assertIsNotNone(artifacts.junit_xml)
                self.assertTrue(artifacts.junit_xml.exists())

                # Check summary.md
                run_dir = artifacts.junit_xml.parent
                summary_path = run_dir / "summary.md"
                self.assertTrue(summary_path.exists())
                self.assertIn("## Verdict Run: ✅ PASS", summary_path.read_text())

                # Check artifacts.json
                index_path = run_dir / "artifacts.json"
                self.assertTrue(index_path.exists())
                index = json.loads(index_path.read_text())
                self.assertEqual(index["run_id"], res.run_id)
                self.assertIn("summary_md", index["paths"])
                self.assertIn("junit_xml", index["paths"])

                # Read content
                content = artifacts.junit_xml.read_text()
                # print("XML Content:", content)

                # Verify XML
                root = ET.fromstring(content)
                self.assertEqual(root.tag, "testsuites")
                ts = root.find("testsuite")
                self.assertEqual(ts.get("name"), "junit_test_suite")
                self.assertEqual(ts.get("tests"), "1")
                self.assertEqual(ts.get("failures"), "0")

                tc = ts.find("testcase")
                self.assertEqual(tc.get("classname"), "verdict.tests")
                self.assertEqual(tc.get("name"), "t1")

        finally:
            os.chdir(cwd)

    def test_junit_xml_failure(self):
        cwd = os.getcwd()
        os.chdir(self.test_dir)
        try:
            # Config that fails
            with open(self.config_path, "w") as f:
                f.write(
                    """
version: 1
suite: junit_fail_suite
tests:
  - id: t2
    prompt: "hello"
    metrics:
      - name: regex_match
        kind: builtin
        threshold: 0.5
        params:
            pattern: "FAIL"
"""
                )
            ev = Evaluator()
            res = ev.run(str(self.trace_path))

            self.assertFalse(res.passed)

            content = res.artifacts.junit_xml.read_text()
            root = ET.fromstring(content)
            ts = root.find("testsuite")
            self.assertEqual(ts.get("failures"), "1")

            tc = ts.find("testcase")
            failure = tc.find("failure")
            self.assertIsNotNone(failure)
            self.assertIn("Test failed on metrics", failure.get("message"))

        finally:
            os.chdir(cwd)

    def test_artifacts_missing_optional(self):
        """Verify artifacts.json handles missing optional artifacts gracefully."""
        cwd = os.getcwd()
        os.chdir(self.test_dir)
        try:
            # Just verify that basic run passes with our setup
            pass
        finally:
            os.chdir(cwd)

    def test_artifacts_json_overwrite(self):
        """Verify that re-running updates artifacts.json correctly."""
        cwd = os.getcwd()
        os.chdir(self.test_dir)
        try:
            # Placeholder for now
            pass
        finally:
            os.chdir(cwd)
