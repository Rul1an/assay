"""Retrospective black-box protection for the published retained-byte recount.

No producer import. Expectations come from the published finding, not the
reproducer's classification tables. These tests do not execute CA or the adapter.
"""
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
PACKAGE = HERE / "record/adapter-selection"
TOOL = PACKAGE / "independent_reproducer.py"


def recount(root):
    return subprocess.run([sys.executable, "-I", "-S", "-B", str(TOOL), str(root)],
                          capture_output=True, timeout=30, check=False)


class RetainedRecount(unittest.TestCase):
    def test_recorded_counts_and_four_separate_reports(self):
        result = recount(PACKAGE)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stderr, b"")
        report = json.loads(result.stdout)
        self.assertEqual(set(report), {"classes", "four_reports", "invocations",
                                       "matrix_cells", "unique_applications"})
        self.assertEqual((report["invocations"], report["unique_applications"],
                          report["matrix_cells"]), (294, 19, 196))
        self.assertEqual(report["classes"], {
            "M": {"U": 102, "U*": 2, "AI": 7, "RO": 17,
                  "GR-P": 6, "GR-A": 6},
            "C": {"U": 31, "AI": 17, "SM": 6, "RO": 2},
        })
        expected = {
            "V1": ("LATE", "r", 3, "not_established"),
            "V2": ("LATE", "r2", 5, "established"),
            "V3": ("LATE-CONFLICT", "r", 4, "established"),
            "V4": ("LATE-CONFLICT", "r2", 7, "not_established"),
        }
        self.assertEqual(set(report["four_reports"]), set(expected))
        commitments = []
        for key, (case, name, sequence, status) in expected.items():
            row = report["four_reports"][key]
            self.assertEqual(row["status"], status)
            self.assertEqual(row["selected"], {"case_id": case,
                "evidence_operation_id": "op", "report_id": name, "report_seq": sequence})
            self.assertRegex(row["commitment"], r"^[0-9a-f]{64}$")
            commitments.append(row["commitment"])
        self.assertEqual(len(set(commitments)), 4)

    def test_changed_retained_bytes_never_print_a_success_summary(self):
        # Each copy starts valid. Mutate a used stdout blob, receipt digest or final.
        for kind in ("blob", "receipt", "final"):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                shutil.copytree(PACKAGE / "files", root / "files")
                baseline = recount(root)
                self.assertEqual(baseline.returncode, 0, baseline.stderr)
                store = root / "files/mount/run/ca-out/S1/926707ac-1d9c-4e25-922c-9e03e934e418"
                receipt_path = store / "receipt-000000.json"
                receipt = json.loads(receipt_path.read_bytes())
                if kind == "blob":
                    blob = store / "blobs" / receipt["stdout_sha256"].removeprefix("sha256:")
                    data = blob.read_bytes()
                    blob.write_bytes(bytes([data[0] ^ 1]) + data[1:])
                elif kind == "receipt":
                    receipt["stdout_sha256"] = "sha256:" + "0" * 64
                    receipt_path.write_text(json.dumps(receipt), encoding="utf-8")
                else:
                    final = store / "final.json"
                    final.write_bytes(final.read_bytes() + b" ")
                broken = recount(root)
                self.assertNotEqual(broken.returncode, 0)
                self.assertEqual(broken.stdout, b"")

    def test_changed_population_or_declared_result_never_prints_a_summary(self):
        # The recount's closed set is the pinned finals' slots over fixed receipt
        # ranges. A missing or repeated member, or a changed result.json that
        # carries the declared matrices, must refuse rather than print counts.
        store = "files/mount/run/ca-out/S1/926707ac-1d9c-4e25-922c-9e03e934e418"
        for kind in ("missing", "duplicate", "result"):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                shutil.copytree(PACKAGE / "files", root / "files")
                if kind == "missing":
                    (root / store / "receipt-000139.json").unlink()
                elif kind == "duplicate":
                    first = (root / store / "receipt-000000.json").read_bytes()
                    (root / store / "receipt-000001.json").write_bytes(first)
                else:
                    result = root / "files/mount/run/result.json"
                    result.write_bytes(result.read_bytes() + b"\n")
                broken = recount(root)
                self.assertNotEqual(broken.returncode, 0)
                self.assertEqual(broken.stdout, b"")


if __name__ == "__main__":
    unittest.main()
