"""Literal contract cases plus targeted boundary tests; no external executions."""

import copy
import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from closure_check import check

CASES = json.loads(Path(__file__).with_name("CASES.json").read_text())["cases"]


def sha(s):
    return hashlib.sha256(s.encode()).hexdigest()


def fixture(index=0):
    return copy.deepcopy(CASES[index]["input"])


def trace(x, rows):
    x["trace_utf8"] = "".join(json.dumps(r, sort_keys=True) + "\n" for r in rows)
    x["context"]["trace_sha256"] = sha(x["trace_utf8"])
    return x


def rows(x):
    return [json.loads(r) for r in x["trace_utf8"].splitlines()]


def observation(x, seq, state, action="a", attempt=1):
    b = dict(seq=seq, session="s1", action_id=action, attempt=attempt, state=state)
    raw = json.dumps(b) + "\n"
    digest = sha(raw)
    x["blobs"][digest] = raw
    return dict(b, kind="observation", evidence_sha256=digest)


class BoundaryTests(unittest.TestCase):
    def expect(self, x, outcome, reason):
        result = check(json.dumps(x).encode())
        self.assertEqual((result["outcome"], result["reason"]), (outcome, reason))
        return result

    def test_frozen_inventory_and_native_states(self):
        self.assertEqual([c["id"] for c in CASES], [f"RC{i:02}" for i in range(1, 13)])
        r = self.expect(fixture(1), "supported", "all_selected_terminal")
        self.assertIn("failed", [x.get("state") for x in r["native"]])
        r = self.expect(fixture(11), "contradicted", "pending_at_cutoff")
        self.assertIn("producer_claim", [x["kind"] for x in r["native"]])

    def test_pending_requires_source_not_coverage(self):
        x = fixture(11)
        x["context"]["continuity"] = False
        self.expect(x, "contradicted", "pending_at_cutoff")
        x["context"]["source_class"] = "producer"
        self.expect(x, "not_established", "source_unaccepted")

    def test_missing_pending_and_early_pending_are_unknown(self):
        x = fixture(11)
        x["blobs"] = {}
        self.expect(x, "not_established", "evidence_missing")
        x = fixture()
        trace(x, [rows(x)[0], observation(x, 3, "pending")])
        self.expect(x, "not_established", "terminal_not_observed")

    def test_subset_excludes_other_pending_but_keeps_native(self):
        x = fixture()
        r = rows(x)
        trace(
            x,
            [
                r[0],
                dict(seq=2, session="s1", action_id="b", attempt=1, kind="register"),
                r[1],
                observation(x, 4, "pending", "b"),
            ],
        )
        result = self.expect(x, "supported", "all_selected_terminal")
        self.assertEqual(result["selection"], x["claim"]["selection"])
        self.assertTrue(any(r["action_id"] == "b" for r in result["native"]))
        x["claim"]["selection"].append(dict(action_id="b", attempt=1))
        self.expect(x, "contradicted", "pending_at_cutoff")

    def test_retry_cannot_complete_selected_attempt(self):
        x = fixture()
        r = rows(x)
        trace(
            x,
            [
                r[0],
                dict(r[0], seq=2, attempt=2),
                observation(x, 3, "completed", attempt=2),
            ],
        )
        self.expect(x, "not_established", "terminal_not_observed")

    def test_duplicate_selection_refuses(self):
        x = fixture()
        x["claim"]["selection"] *= 2
        self.expect(x, "no_claim", "ambiguous_selection")

    def test_missing_late_blob_not_in_bound(self):
        x = fixture(5)
        x["blobs"] = {}
        self.expect(x, "not_established", "terminal_not_observed")

    def test_all_coverage_conditions(self):
        for field, value in [
            ("continuity", False),
            ("coverage_through_seq", 3),
            ("allowed_channels", []),
            ("observed_channels", []),
        ]:
            with self.subTest(field=field):
                x = fixture()
                x["context"][field] = value
                self.expect(x, "not_established", "coverage_incomplete")

    def test_conflicting_history_after_cutoff_refuses(self):
        x = fixture()
        trace(x, rows(x) + [observation(x, 5, "pending")])
        self.expect(x, "no_claim", "invalid_input")

    def test_orphan_empty_selection_and_late_registration_refuse(self):
        x = fixture()
        trace(x, rows(x)[1:])
        self.expect(x, "no_claim", "invalid_input")
        x = fixture()
        x["claim"]["selection"] = []
        self.expect(x, "no_claim", "invalid_input")
        x = fixture()
        trace(x, [dict(rows(x)[0], seq=5)])
        self.expect(x, "no_claim", "invalid_input")

    def test_blob_binding_and_unreferenced_blobs(self):
        x = fixture()
        x["blobs"]["0" * 64] = next(iter(x["blobs"].values()))
        self.expect(x, "no_claim", "binding_mismatch")
        x = fixture()
        r = rows(x)
        b = json.loads(next(iter(x["blobs"].values())))
        b["state"] = "failed"
        raw = json.dumps(b)
        x["blobs"] = {sha(raw): raw}
        r[1]["evidence_sha256"] = sha(raw)
        trace(x, r)
        self.expect(x, "no_claim", "binding_mismatch")
        x = fixture()
        x["blobs"][sha("{}")] = "{}"
        self.expect(x, "no_claim", "invalid_input")

    def test_unknown_kind_preempts_hash_and_retains_both(self):
        x = fixture()
        r = rows(x)
        r[1]["kind"] = "novel"
        trace(x, r)
        x["context"]["trace_sha256"] = "0" * 64
        result = self.expect(x, "no_claim", "unsupported_profile")
        self.assertIn("binding_mismatch", [r["reason"] for r in result["refusals"]])

    def test_malformed_types_and_fields(self):
        for change in ["bool", "float", "extra", "seq", "channel"]:
            x = fixture()
            if change == "bool":
                x["claim"]["selection"][0]["attempt"] = True
            if change == "float":
                x["claim"]["cutoff_seq"] = 4.0
            if change == "extra":
                x["extra"] = True
            if change == "seq":
                trace(x, [dict(r, seq=1) for r in rows(x)])
            if change == "channel":
                x["context"]["allowed_channels"] *= 2
            with self.subTest(change=change):
                self.expect(x, "no_claim", "invalid_input")

    def test_empty_profile_is_invalid_not_an_unknown_profile(self):
        x = fixture()
        x["profile"] = ""
        self.expect(x, "no_claim", "invalid_input")

    def test_cli_preserves_result_and_distinguishes_usage_and_io(self):
        command = [sys.executable, str(Path(__file__).with_name("closure_check.py"))]
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "input.json"
            for x, expected in [(fixture(), "supported"), (fixture(9), "no_claim")]:
                path.write_text(json.dumps(x))
                run = subprocess.run(
                    command + [str(path)], capture_output=True, text=True, timeout=10
                )
                self.assertEqual(run.returncode, 0, run.stderr)
                self.assertEqual(json.loads(run.stdout)["outcome"], expected)
            path.unlink()
            self.assertEqual(
                subprocess.run(
                    command + [str(path)], capture_output=True, timeout=10
                ).returncode,
                66,
            )
        self.assertEqual(
            subprocess.run(command, capture_output=True, timeout=10).returncode, 64
        )

    def test_strict_json_and_utf8(self):
        for raw in [b'{"x":1,"x":2}', b"NaN", b"\xff", b"[]", b"null", b"1e999"]:
            with self.subTest(raw=raw):
                r = check(raw)
                self.assertEqual(
                    (r["outcome"], r["reason"]), ("no_claim", "invalid_input")
                )
        x = fixture()
        x["trace_utf8"] = "\ud800"
        self.expect(x, "no_claim", "invalid_input")

    def test_json_whitespace_is_hashed_verbatim(self):
        x = fixture()
        r = rows(x)
        raw = " \t" + next(iter(x["blobs"].values())) + "\r\n"
        x["blobs"] = {sha(raw): raw}
        r[1]["evidence_sha256"] = sha(raw)
        trace(x, r)
        self.expect(x, "supported", "all_selected_terminal")
        x["trace_utf8"] = x["trace_utf8"].rstrip("\n")
        x["context"]["trace_sha256"] = sha(x["trace_utf8"])
        self.expect(x, "no_claim", "invalid_input")


class FrozenCases(unittest.TestCase):
    pass


def make_case(c):
    def test(self):
        result = check(json.dumps(c["input"]).encode())
        self.assertEqual({k: result[k] for k in ("outcome", "reason")}, c["expected"])

    return test


for case in CASES:
    setattr(FrozenCases, "test_" + case["id"], make_case(case))

if __name__ == "__main__":
    unittest.main()
