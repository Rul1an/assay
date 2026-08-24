#!/usr/bin/env python3
import json
import subprocess
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CLASSIFIER = ROOT / "scripts" / "interop" / "classify_sep2828_fallback.py"


class FallbackClassificationTests(unittest.TestCase):
    MAX_REPORT_BYTES = 1024 * 1024

    def classify(self, report: dict, decision: dict) -> tuple[int, dict]:
        return self.classify_raw(json.dumps(report), decision)

    def classify_raw(self, report: str, decision: dict) -> tuple[int, dict | None]:
        self.assertTrue(CLASSIFIER.is_file(), f"missing classifier: {CLASSIFIER}")
        projection = decision.get("backLink", {}).get("fallbackProjection")
        completed = subprocess.run(
            ["python3", str(CLASSIFIER), json.dumps(projection)],
            input=report,
            check=False,
            capture_output=True,
            text=True,
        )
        result = json.loads(completed.stdout) if completed.stdout else None
        return completed.returncode, result

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
                {"id": check_id, "ok": check_id not in false_checks}
                for check_id in sorted(required_true | set(false_checks))
            ],
        }

    @staticmethod
    def missing_params_report() -> dict:
        return {
            "ok": False,
            "binding": {
                "mode": "request_envelope",
                "projection": "assay.fallback_projection.v0",
                "digest_source": "request_envelope_named_projection_jcs",
                "digest": None,
            },
            "checks": [
                {"id": "fallback_projection_missing_params", "ok": False},
                {"id": "decision_request_envelope_nonce_present", "ok": True},
                {"id": "decision_outcome_backlink_match", "ok": True},
                {"id": "outcome_decision_digest_match", "ok": True},
                {"id": "result_commitment_projection_digest_match", "ok": True},
            ],
            "claims_not_made": ["fallback_call_parameter_binding"],
        }

    @staticmethod
    def decision(projection: object = "tools_call_params_plus_meta_authorization_binding_v1") -> dict:
        return {"backLink": {"fallbackProjection": projection}}

    def test_exact_projection_mismatch_is_documented_non_reproduction(self) -> None:
        rc, result = self.classify(self.missing_params_report(), self.decision())

        self.assertEqual(rc, 0)
        self.assertEqual(result["classification"], "documented_non_reproduction")
        self.assertEqual(result["assay_projection"], "assay.fallback_projection.v0")
        self.assertEqual(
            result["upstream_projection"],
            "tools_call_params_plus_meta_authorization_binding_v1",
        )

    def test_historical_digest_mismatch_shape_is_now_drift(self) -> None:
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
        self.assertEqual(result["classification"], "diverged")

    def test_unresolved_projection_must_not_publish_a_digest(self) -> None:
        for replacement in ("missing", "sha256:unresolved"):
            with self.subTest(replacement=replacement):
                report = self.missing_params_report()
                if replacement == "missing":
                    del report["binding"]["digest"]
                else:
                    report["binding"]["digest"] = replacement

                rc, result = self.classify(report, self.decision())

                self.assertEqual(rc, 0)
                self.assertEqual(result["classification"], "diverged")

    def test_missing_params_is_the_only_accepted_shape_reason(self) -> None:
        report = self.missing_params_report()
        report["checks"][0]["id"] = "fallback_projection_invalid_params"

        rc, result = self.classify(report, self.decision())

        self.assertEqual(rc, 0)
        self.assertEqual(result["classification"], "diverged")

    def test_unresolved_projection_requires_parameter_binding_nonclaim(self) -> None:
        report = self.missing_params_report()
        report["claims_not_made"] = []

        rc, result = self.classify(report, self.decision())

        self.assertEqual(rc, 0)
        self.assertEqual(result["classification"], "diverged")

    def test_unresolved_projection_omits_checks_that_require_a_resolved_preimage(self) -> None:
        for check_id in (
            "fallback_projection_binding_present",
            "decision_request_envelope_digest_match",
            "outcome_request_envelope_digest_match",
        ):
            with self.subTest(check_id=check_id):
                report = self.missing_params_report()
                # Even a green dependent check is drift: it claims a preimage existed.
                report["checks"].append({"id": check_id, "ok": True})

                rc, result = self.classify(report, self.decision())

                self.assertEqual(rc, 0)
                self.assertEqual(result["classification"], "diverged")

    def test_each_remaining_pairing_check_is_required(self) -> None:
        for check_id in (
            "decision_request_envelope_nonce_present",
            "decision_outcome_backlink_match",
            "outcome_decision_digest_match",
            "result_commitment_projection_digest_match",
        ):
            with self.subTest(check_id=check_id):
                report = self.missing_params_report()
                report["checks"] = [
                    check for check in report["checks"] if check["id"] != check_id
                ]

                rc, result = self.classify(report, self.decision())

                self.assertEqual(rc, 0)
                self.assertEqual(result["classification"], "diverged")

    def test_check_order_does_not_change_the_classification(self) -> None:
        report = self.missing_params_report()
        report["checks"].reverse()

        rc, result = self.classify(report, self.decision())

        self.assertEqual(rc, 0)
        self.assertEqual(result["classification"], "documented_non_reproduction")

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
        rc, result = self.classify(self.missing_params_report(), self.decision(None))

        self.assertEqual(rc, 0)
        self.assertEqual(result["classification"], "diverged")

    def test_missing_overall_verdict_is_a_divergence(self) -> None:
        report = self.missing_params_report()
        del report["ok"]

        rc, result = self.classify(report, self.decision())

        self.assertEqual(rc, 0)
        self.assertEqual(result["classification"], "diverged")

    def test_each_binding_field_is_load_bearing(self) -> None:
        mutations = {
            "mode": "attestation",
            "projection": "another.assay.projection.v0",
            "digest_source": "whole_request_envelope_jcs",
        }
        for field, replacement in mutations.items():
            with self.subTest(field=field):
                report = self.missing_params_report()
                report["binding"][field] = replacement

                rc, result = self.classify(report, self.decision())

                self.assertEqual(rc, 0)
                self.assertEqual(result["classification"], "diverged")

    def test_different_upstream_projection_is_a_divergence(self) -> None:
        rc, result = self.classify(
            self.missing_params_report(),
            self.decision("another_named_projection_v2"),
        )

        self.assertEqual(rc, 0)
        self.assertEqual(result["classification"], "diverged")

    def test_duplicate_check_id_is_a_divergence(self) -> None:
        report = self.missing_params_report()
        report["checks"].insert(
            0,
            {"id": "fallback_projection_missing_params", "ok": True},
        )

        rc, result = self.classify(report, self.decision())

        self.assertEqual(rc, 0)
        self.assertEqual(result["classification"], "diverged")

    def test_malformed_extra_check_is_a_divergence(self) -> None:
        for malformed in (
            {"id": "extra_without_ok"},
            {"id": 7, "ok": False},
            "not-a-check",
        ):
            with self.subTest(malformed=malformed):
                report = self.missing_params_report()
                report["checks"].append(malformed)

                rc, result = self.classify(report, self.decision())

                self.assertEqual(rc, 0)
                self.assertEqual(result["classification"], "diverged")

    def test_oversized_report_fails_closed(self) -> None:
        oversized = json.dumps({"padding": "x" * self.MAX_REPORT_BYTES})

        rc, result = self.classify_raw(oversized, self.decision())

        self.assertEqual(rc, 2)
        self.assertIsNone(result)


if __name__ == "__main__":
    unittest.main()
