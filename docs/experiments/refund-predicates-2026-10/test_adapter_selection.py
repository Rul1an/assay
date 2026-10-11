"""Producer regression tests, separate from independent retained-byte recount.

Calls the real pure selection function with the published fixture data. It
does not execute front.py, the native reader, CA or a subprocess supervisor.
"""
import base64
import copy
import importlib.util
import json
from pathlib import Path
import unittest

PACKAGE = Path(__file__).resolve().parent / "record/adapter-selection"
SOURCE = PACKAGE / "source/subject/component/py/projector.py"
spec = importlib.util.spec_from_file_location("selection_subject", SOURCE)
projector = importlib.util.module_from_spec(spec)
spec.loader.exec_module(projector)


def fixture(name):
    vector = json.loads((PACKAGE / "source/subject/vectors" / (name + ".json")).read_bytes())
    evidence = json.loads(base64.b64decode(vector["evidence_b64"]))
    packet = json.loads(base64.b64decode(evidence["packet"]["bytes_b64"]))
    native = json.loads(base64.b64decode(evidence["native_output"]["bytes_b64"]))
    return evidence, packet, native


def select(rows, events, name):
    return projector.select_report(rows=rows, events=events, report_id=name,
                                   axis="definite_support")


class ReportSelection(unittest.TestCase):
    def test_four_named_reports_keep_their_native_results(self):
        expected = {"V1": ("r", 3, "not_established", "unsupported_definite_report"),
                    "V2": ("r2", 5, "established", "terminal_support"),
                    "V3": ("r", 4, "established", "terminal_support"),
                    "V4": ("r2", 7, "not_established", "contrary_terminal_evidence")}
        for vector, (name, seq, status, reason) in expected.items():
            with self.subTest(vector=vector):
                _, packet, native = fixture(vector)
                row = select(native["claims"], packet["events"], name)
                self.assertEqual((row["report_id"], row["report_seq"]), (name, seq))
                result = projector.native_for(profile=projector.C3, row=row, axis="definite_support")
                self.assertEqual(result["reason"], reason)
                self.assertEqual(projector.map_status(native=result, has_applicability=True), status)

    def test_missing_or_duplicate_report_is_refused(self):
        _, packet, native = fixture("V1")
        rows, events = native["claims"], packet["events"]
        duplicate_events = events + [copy.deepcopy(next(e for e in events if e.get("id") == "r"))]
        for candidates, observations, name in (
                (rows, events, None), (rows, events, "absent"),
                (rows + [copy.deepcopy(rows[0])], events, "r"),
                (rows, duplicate_events, "r")):
            with self.subTest(name=name, rows=len(candidates), events=len(observations)):
                with self.assertRaises(projector.NoClaim) as caught:
                    select(candidates, observations, name)
                self.assertEqual(caught.exception.reason, "subject_not_found")

    def test_wrong_sequence_is_refused(self):
        _, packet, native = fixture("V1")
        native["claims"][0]["report_seq"] = 4
        with self.assertRaises(projector.NoClaim) as caught:
            select(native["claims"], packet["events"], "r")
        self.assertEqual(caught.exception.reason, "subject_binding_mismatch")


if __name__ == "__main__":
    unittest.main()
