import hashlib
import json
import os
import unittest
from pathlib import Path

from adapter import evaluate

HERE = Path(__file__).resolve().parent


class AdapterCases(unittest.TestCase):
    def test_frozen_native_cases(self):
        for fixture in sorted((HERE / "fixtures").glob("*.json")):
            with self.subTest(case=fixture.stem):
                case = json.loads(fixture.read_text())
                result = evaluate(case["input"], os.environ["CLOSURE_PARSER"])
                self.assertEqual(
                    [result["outcome"], result["reason"]], case["expected"]
                )

    def test_exact_source_lines_retained(self):
        case = json.loads((HERE / "fixtures/02-tool-failure.json").read_text())
        result = evaluate(case["input"], os.environ["CLOSURE_PARSER"])
        for row, line in zip(
            result["native"],
            case["input"]["trace_utf8"].splitlines(keepends=True),
            strict=True,
        ):
            self.assertEqual(row.get("source_line_utf8"), line)
            self.assertEqual(
                row.get("line_sha256"), hashlib.sha256(line.encode()).hexdigest()
            )
        self.assertTrue(result["native"][1]["raw"]["result"]["isError"])

    def test_refusal_carries_claim_scope(self):
        case = json.loads((HERE / "fixtures/14-hash.json").read_text())
        result = evaluate(case["input"], os.environ["CLOSURE_PARSER"])
        self.assertEqual(result.get("claim_scope"), "assay-declared-mcp-call-result")


if __name__ == "__main__":
    unittest.main()
