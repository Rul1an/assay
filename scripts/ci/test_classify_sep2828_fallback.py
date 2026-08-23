#!/usr/bin/env python3
import json
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CLASSIFIER = ROOT / "scripts" / "interop" / "classify_sep2828_fallback.py"


class FallbackClassificationTests(unittest.TestCase):
    def classify(self, report: dict, decision: dict) -> tuple[int, dict]:
        self.assertTrue(CLASSIFIER.is_file(), f"missing classifier: {CLASSIFIER}")
        with tempfile.TemporaryDirectory() as tmp:
            report_path = Path(tmp) / "report.json"
            decision_path = Path(tmp) / "decision.json"
            report_path.write_text(json.dumps(report))
            decision_path.write_text(json.dumps(decision))
            completed = subprocess.run(
                ["python3", str(CLASSIFIER), str(report_path), str(decision_path)],
                check=False,
                capture_output=True,
                text=True,
            )
        return completed.returncode, json.loads(completed.stdout)

    @staticmethod
    def report(*, ok: bool, false_checks: list[str]) -> dict:
        required_true = {
            "fallback_projection_binding_present",
            "decision_request_envelope_nonce_present",
            "decision_outcome_backlink_match",
            "outcome_decision_digest_match",
            "result_commitment_projection_digest_match",
        }
        return {
            "ok": ok,
            "binding": {
                "mode": "request_envelope",
                "projection": "assay.fallback_projection.v0",
                "digest_source": "request_envelope_named_projection_jcs",
            },
            "checks": [
                {"id": check_id, "ok": check_id in required_true}
                for check_id in sorted(required_true | set(false_checks))
            ],
        }

    @staticmethod
    def decision(projection: object = "tools_call_params_plus_meta_authorization_binding_v1") -> dict:
        return {"backLink": {"fallbackProjection": projection}}

    def test_exact_projection_mismatch_is_documented_non_reproduction(self) -> None:
        rc, result = self.classify(
            self.report(
                ok=False,
                false_checks=[
                    "decision_request_envelope_digest_match",
                    "outcome_request_envelope_digest_match",
                ],
            ),
            self.decision(),
        )

        self.assertEqual(rc, 0)
        self.assertEqual(result["classification"], "documented_non_reproduction")
        self.assertEqual(result["assay_projection"], "assay.fallback_projection.v0")
        self.assertEqual(
            result["upstream_projection"],
            "tools_call_params_plus_meta_authorization_binding_v1",
        )

    def test_unexpected_failure_is_a_divergence(self) -> None:
        rc, result = self.classify(
            self.report(ok=False, false_checks=["outcome_decision_digest_match"]),
            self.decision(),
        )

        self.assertEqual(rc, 0)
        self.assertEqual(result["classification"], "diverged")

    def test_success_is_reported_as_reproduced_drift(self) -> None:
        rc, result = self.classify(self.report(ok=True, false_checks=[]), self.decision())

        self.assertEqual(rc, 0)
        self.assertEqual(result["classification"], "reproduced")

    def test_missing_upstream_projection_is_a_divergence(self) -> None:
        rc, result = self.classify(
            self.report(
                ok=False,
                false_checks=[
                    "decision_request_envelope_digest_match",
                    "outcome_request_envelope_digest_match",
                ],
            ),
            self.decision(None),
        )

        self.assertEqual(rc, 0)
        self.assertEqual(result["classification"], "diverged")


if __name__ == "__main__":
    unittest.main()
