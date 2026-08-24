#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = Path(
    os.environ.get(
        "ASSAY_SEP2828_REPRO_SCRIPT",
        ROOT / "scripts" / "interop" / "reproduce-sep2828-decision-pairing.sh",
    )
)


class ReproductionFunnelTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.upstream = self.root / "upstream"
        self.invocations = self.root / "invocations.jsonl"
        self.assay = self.root / "assay"
        self._write_vectors()
        self._write_fake_assay()

    def tearDown(self) -> None:
        self.temp.cleanup()

    def _write(self, case: str, name: str, value: dict) -> None:
        destination = self.upstream / case / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(json.dumps(value), encoding="utf-8")

    def _write_vectors(self) -> None:
        cases = {
            "valid_pair_allow_executed": (True, True),
            "decision_only_escalate": (True, False),
            "substituted_attestation_backlink": (True, True),
            "substituted_pairing_nonce": (True, True),
            "substituted_decision_under_shared_attestation": (True, True),
        }
        for case, (attestation, receipt) in cases.items():
            self._write(case, "decision.json", {"case": case})
            if attestation:
                self._write(case, "attestation.json", {"case": case})
            if receipt:
                self._write(case, "receipt.json", {"case": case})
        self._write("supersession_equal_decidedat_tie", "decision_a.json", {"case": "supersession"})
        self._write("supersession_equal_decidedat_tie", "decision_b.json", {"case": "supersession"})
        self._write("fallback_envelope_binding", "request_envelope.json", {"request": "fallback"})
        self._write(
            "fallback_envelope_binding",
            "decision.json",
            {
                "case": "fallback",
                "backLink": {
                    "fallbackProjection": "tools_call_params_plus_meta_authorization_binding_v1"
                },
            },
        )
        self._write("fallback_envelope_binding", "receipt.json", {"case": "fallback"})

    def _write_fake_assay(self) -> None:
        self.assay.write_text(
            textwrap.dedent(
                """\
                #!/usr/bin/env python3
                import json, os, pathlib, sys

                args = sys.argv[1:]
                if args == ["--version"]:
                    print("assay 5.4.0-test")
                    raise SystemExit(0)
                if os.environ.get("FAKE_ASSAY_FAIL") == "1":
                    raise SystemExit(17)
                log = pathlib.Path(os.environ["ASSAY_TEST_INVOCATIONS"])
                with log.open("a", encoding="utf-8") as handle:
                    handle.write(json.dumps(args) + "\\n")
                if "verify-mcp-supersession" in args:
                    print(json.dumps({"groups": [{"verdict": "ambiguous", "reason_code": "supersession_ambiguous_missing_sequence"}]}))
                    raise SystemExit(0)
                decision = json.load(open(args[args.index("--decision") + 1], encoding="utf-8"))
                case = decision["case"]
                expected = {
                    "valid_pair_allow_executed": (True, {
                        "decision_outcome_backlink_match": True,
                        "outcome_decision_digest_match": True,
                        "result_commitment_projection_digest_match": True,
                    }),
                    "decision_only_escalate": (True, {
                        "decision_attestation_digest_match": True,
                        "outcome_absent": True,
                    }),
                    "substituted_attestation_backlink": (False, {
                        "decision_attestation_digest_match": True,
                        "outcome_attestation_digest_match": False,
                        "decision_outcome_backlink_match": False,
                    }),
                    "substituted_pairing_nonce": (False, {
                        "outcome_attestation_digest_match": True,
                        "outcome_attestation_nonce_match": False,
                        "decision_outcome_backlink_match": False,
                    }),
                    "substituted_decision_under_shared_attestation": (False, {
                        "decision_outcome_backlink_match": True,
                        "outcome_decision_digest_match": False,
                    }),
                }
                if case == "fallback":
                    required = {
                        "fallback_projection_missing_params": False,
                        "decision_request_envelope_nonce_present": True,
                        "decision_outcome_backlink_match": True,
                        "outcome_decision_digest_match": True,
                        "result_commitment_projection_digest_match": True,
                    }
                    ok = os.environ.get("FAKE_FALLBACK_REPRODUCED") == "1"
                    if ok:
                        required = {
                            "fallback_projection_binding_present": True,
                            "decision_request_envelope_nonce_present": True,
                            "decision_request_envelope_digest_match": True,
                            "decision_outcome_backlink_match": True,
                            "outcome_request_envelope_digest_match": True,
                            "outcome_decision_digest_match": True,
                            "result_commitment_projection_digest_match": True,
                        }
                    report = {
                        "ok": ok,
                        "binding": {
                            "mode": "request_envelope",
                            "projection": "assay.fallback_projection.v0",
                            "digest_source": "request_envelope_named_projection_jcs",
                            "digest": "sha256:future" if ok else None,
                        },
                        "checks": [{"id": key, "ok": value} for key, value in required.items()],
                        "claims_not_made": [] if ok else ["fallback_call_parameter_binding"],
                    }
                    if os.environ.get("FAKE_FALLBACK_OMIT_OK") == "1":
                        del report["ok"]
                    print(json.dumps(report))
                    raise SystemExit(0 if ok else 2)
                ok, checks = expected[case]
                print(json.dumps({"ok": ok, "checks": [{"id": key, "ok": value} for key, value in checks.items()]}))
                raise SystemExit(0 if ok else 2)
                """
            ),
            encoding="utf-8",
        )
        self.assay.chmod(0o755)

    def run_script(self, **extra_env: str) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        env.update(
            {
                "ASSAY_INTEROP_UPSTREAM_BASE": self.upstream.as_uri(),
                "ASSAY_TEST_INVOCATIONS": str(self.invocations),
                **extra_env,
            }
        )
        return subprocess.run(
            ["bash", str(SCRIPT), str(self.assay)],
            cwd=ROOT,
            env=env,
            check=False,
            capture_output=True,
            text=True,
        )

    def test_full_funnel_executes_named_fallback_and_records_six_zero_one(self) -> None:
        completed = self.run_script()

        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertIn(
            "not reproduced (unsupported named-projection envelope shape)",
            completed.stdout,
        )
        self.assertIn("reproduced: 6   diverged: 0   documented non-reproduction: 1", completed.stdout)
        invocations = [json.loads(line) for line in self.invocations.read_text().splitlines()]
        fallback = [args for args in invocations if "--request-envelope" in args]
        self.assertEqual(len(fallback), 1)
        self.assertIn("--fallback-projection", fallback[0])
        self.assertEqual(fallback[0][fallback[0].index("--fallback-projection") + 1], "named")

    def test_classifier_result_controls_the_final_count(self) -> None:
        completed = self.run_script(FAKE_FALLBACK_REPRODUCED="1")

        self.assertEqual(completed.returncode, 1)
        self.assertIn("now reproduced; update the recorded result", completed.stdout)

    def test_missing_fallback_overall_verdict_is_drift(self) -> None:
        completed = self.run_script(FAKE_FALLBACK_OMIT_OK="1")

        self.assertEqual(completed.returncode, 1)
        self.assertIn("unexpected fallback failure", completed.stdout)

    def test_pairing_tool_failure_is_setup_failure(self) -> None:
        completed = self.run_script(FAKE_ASSAY_FAIL="1")

        self.assertEqual(completed.returncode, 2)
        self.assertIn("assay exited 17", completed.stderr)

    def test_oversized_vector_is_rejected_before_publish(self) -> None:
        oversized = self.upstream / "valid_pair_allow_executed" / "decision.json"
        oversized.write_bytes(b"x" * (1024 * 1024 + 1))

        completed = self.run_script()

        self.assertEqual(completed.returncode, 2)
        self.assertIn("exceeds ceiling", completed.stderr)


if __name__ == "__main__":
    unittest.main()
